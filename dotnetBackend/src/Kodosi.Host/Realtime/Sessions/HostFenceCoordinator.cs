using System.Collections.Concurrent;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed class HostFenceCoordinator(
    ILiveSessionStateDirectory runtimes,
    OperationalMetrics metrics,
    ILogger<HostFenceCoordinator> logger,
    Func<SessionId, byte[], ChannelByteSendQueueWriteOutcome> sendToHost,
    Action<SessionId, CloseReason> disconnectHost)
{
    private const int PreHostPendingFenceCapacity = 128;
    private const int UnacknowledgedFenceCapacity = 1024;

    private readonly ConcurrentDictionary<SessionId, FenceLockEntry> _hostFenceLocks = new();
    private readonly ConcurrentDictionary<SessionId, PendingHostFenceQueue> _preHostFenceQueues = new();
    private readonly ConcurrentDictionary<SessionId, PendingHostFenceQueue> _unacknowledged = new();
    private readonly ConcurrentDictionary<SessionId, Queue<PendingHostFence>> _replayRemaining = new();
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly ILogger<HostFenceCoordinator> _logger = logger;
    private readonly Func<SessionId, byte[], ChannelByteSendQueueWriteOutcome> _sendToHost = sendToHost;
    private readonly Action<SessionId, CloseReason> _disconnectHost = disconnectHost;

    public FenceLockLease AcquireLock(SessionId sessionId)
    {
        while (true)
        {
            var entry = _hostFenceLocks.GetOrAdd(
                sessionId,
                static _ => new FenceLockEntry());
            lock (entry.StateLock)
            {
                if (entry.Retired)
                {
                    _hostFenceLocks.TryRemove(
                        new KeyValuePair<SessionId, FenceLockEntry>(
                            sessionId,
                            entry));
                    continue;
                }

                entry.ReferenceCount++;
                return new FenceLockLease(this, sessionId, entry);
            }
        }
    }

    public void RemovePreHostQueue(SessionId sessionId)
    {
        using var fenceLock = AcquireLock(sessionId);
        lock (fenceLock.SyncRoot)
        {
            _preHostFenceQueues.TryRemove(sessionId, out _);
            _unacknowledged.TryRemove(sessionId, out _);
            _replayRemaining.TryRemove(sessionId, out _);
        }
    }

    public void Acknowledge(SessionId sessionId, string fenceId)
    {
        using var fenceLock = AcquireLock(sessionId);
        lock (fenceLock.SyncRoot)
        {
            if (!_unacknowledged.TryGetValue(sessionId, out var queue))
            {
                return;
            }
            queue.Remove(fenceId);
            if (!queue.HasPending)
            {
                _unacknowledged.TryRemove(
                    new KeyValuePair<SessionId, PendingHostFenceQueue>(sessionId, queue));
            }
        }
    }

    public void ReplayUnacknowledged(SessionId sessionId)
    {
        using var fenceLock = AcquireLock(sessionId);
        lock (fenceLock.SyncRoot)
        {
            if (!_unacknowledged.TryGetValue(sessionId, out var queue))
            {
                _replayRemaining.TryRemove(sessionId, out _);
                return;
            }

            _replayRemaining[sessionId] = new Queue<PendingHostFence>(queue.Snapshot());
            ContinueReplayUnacknowledgedCore(sessionId, queue);
        }
    }

    public void ContinueReplayUnacknowledged(SessionId sessionId)
    {
        using var fenceLock = AcquireLock(sessionId);
        lock (fenceLock.SyncRoot)
        {
            if (!_unacknowledged.TryGetValue(sessionId, out var queue))
            {
                _replayRemaining.TryRemove(sessionId, out _);
                return;
            }

            ContinueReplayUnacknowledgedCore(sessionId, queue);
        }
    }

    private void ContinueReplayUnacknowledgedCore(
        SessionId sessionId,
        PendingHostFenceQueue unacknowledged)
    {
        if (!_replayRemaining.TryGetValue(sessionId, out var remaining))
        {
            return;
        }

        while (remaining.TryPeek(out var fence))
        {
            if (!unacknowledged.Contains(fence.FenceId))
            {
                remaining.Dequeue();
                continue;
            }
            if (SendFence(sessionId, fence) != ChannelByteSendQueueWriteOutcome.Enqueued)
            {
                return;
            }

            remaining.Dequeue();
        }

        _replayRemaining.TryRemove(
            new KeyValuePair<SessionId, Queue<PendingHostFence>>(sessionId, remaining));
    }

    public void NotifyKeyDistributionRequested(SessionId sessionId, bool hasLiveSessionQueues)
    {
        SendOrQueue(sessionId, new PendingHostFence.KeyDistributionRequested(), hasLiveSessionQueues);
    }

    public void NotifyParticipantChanged(
        SessionId sessionId,
        int participantCount,
        string action,
        UserId participantUserId,
        bool hasLiveSessionQueues)
    {
        SendOrQueue(
            sessionId,
            new PendingHostFence.ParticipantChanged(participantUserId, participantCount, action),
            hasLiveSessionQueues);
    }

    public void NotifyAccessRevoked(SessionId sessionId, UserId revokedUserId, bool hasLiveSessionQueues)
    {
        SendOrQueue(
            sessionId,
            new PendingHostFence.AccessRevoked(revokedUserId),
            hasLiveSessionQueues);
    }

    public void FlushPending(SessionId sessionId)
    {
        var ports = _runtimes.TryGet(sessionId);
        if (ports is null)
        {
            return;
        }

        using var fenceLock = AcquireLock(sessionId);
        lock (fenceLock.SyncRoot)
        {
            DrainPreHostFencesToHost(sessionId, ports.Host);
            FlushPendingHostFencesCore(sessionId, ports);
        }
    }

    private ChannelByteSendQueueWriteOutcome SendOrQueue(
        SessionId sessionId,
        PendingHostFence fence,
        bool hasLiveSessionQueues)
    {
        using var fenceLock = AcquireLock(sessionId);
        lock (fenceLock.SyncRoot)
        {
            var ports = _runtimes.TryGet(sessionId);
            if (ports is null && !hasLiveSessionQueues)
            {
                _preHostFenceQueues.TryRemove(sessionId, out _);
                _logger.LogDebug(
                    "Dropped host fence {FenceType} for inactive session {SessionId}",
                    fence.GetType().Name,
                    sessionId);
                return ChannelByteSendQueueWriteOutcome.Closed;
            }

            if (ports is null)
            {
                if (HasPreHostPendingHostFences(sessionId))
                {
                    QueuePreHostPendingHostFence(sessionId, fence, ChannelByteSendQueueWriteOutcome.Closed);
                    return ChannelByteSendQueueWriteOutcome.Closed;
                }

                if (!TryTrackUnacknowledged(sessionId, fence))
                {
                    QueuePreHostPendingHostFence(
                        sessionId,
                        fence,
                        ChannelByteSendQueueWriteOutcome.Full);
                    return ChannelByteSendQueueWriteOutcome.Full;
                }
                var missingRuntimeOutcome = SendFence(sessionId, fence);
                if (missingRuntimeOutcome == ChannelByteSendQueueWriteOutcome.Enqueued)
                {
                    return missingRuntimeOutcome;
                }
                UntrackUnacknowledged(sessionId, fence);

                QueuePreHostPendingHostFence(sessionId, fence, missingRuntimeOutcome);
                return missingRuntimeOutcome;
            }

            DrainPreHostFencesToHost(sessionId, ports.Host);
            if (ports.Host.HasPendingHostFences)
            {
                if (!TryQueueRuntimePendingHostFence(sessionId, ports.Host, fence))
                {
                    return ChannelByteSendQueueWriteOutcome.Full;
                }
                FlushPendingHostFencesCore(sessionId, ports);
                return ChannelByteSendQueueWriteOutcome.Enqueued;
            }

            if (!TryTrackUnacknowledged(sessionId, fence))
            {
                QueuePendingHostFenceAfterFailedSend(
                    sessionId,
                    ports,
                    fence,
                    ChannelByteSendQueueWriteOutcome.Full);
                return ChannelByteSendQueueWriteOutcome.Full;
            }
            var outcome = SendFence(sessionId, fence);
            if (outcome == ChannelByteSendQueueWriteOutcome.Enqueued)
            {
                return outcome;
            }
            UntrackUnacknowledged(sessionId, fence);

            QueuePendingHostFenceAfterFailedSend(sessionId, ports, fence, outcome);
            return outcome;
        }
    }

    private void FlushPendingHostFencesCore(SessionId sessionId, LiveSessionPorts ports)
    {
        while (true)
        {
            var fence = ports.Host.PeekPendingHostFence();
            if (fence is null)
            {
                return;
            }

            if (!TryTrackUnacknowledged(sessionId, fence))
            {
                RecordSendFailure(ChannelByteSendQueueWriteOutcome.Full);
                return;
            }
            var outcome = SendFence(sessionId, fence);
            if (outcome == ChannelByteSendQueueWriteOutcome.Enqueued)
            {
                if (!ports.Host.RemovePendingHostFence(fence))
                {
                    _logger.LogWarning(
                        "Pending host fence {FenceType} for session {SessionId} changed during flush; stopping flush",
                        fence.GetType().Name,
                        sessionId);
                    return;
                }

                continue;
            }
            UntrackUnacknowledged(sessionId, fence);

            RecordSendFailure(outcome);
            _logger.LogInformation(
                "Stopped flushing pending host fence {FenceType} for session {SessionId} after send outcome {Outcome}",
                fence.GetType().Name,
                sessionId,
                outcome);
            return;
        }
    }

    private bool TryTrackUnacknowledged(SessionId sessionId, PendingHostFence fence)
    {
        using var fenceLock = AcquireLock(sessionId);
        lock (fenceLock.SyncRoot)
        {
            var queue = _unacknowledged.GetOrAdd(
                sessionId,
                static _ => new PendingHostFenceQueue(UnacknowledgedFenceCapacity));
            if (queue.TryQueue(fence))
            {
                return true;
            }

            _logger.LogError(
                "Unacknowledged host fence capacity reached for session {SessionId}; disconnecting host fail-closed",
                sessionId);
            _disconnectHost(sessionId, CloseReason.ServerError);
            return false;
        }
    }

    private void UntrackUnacknowledged(SessionId sessionId, PendingHostFence fence)
    {
        using var fenceLock = AcquireLock(sessionId);
        lock (fenceLock.SyncRoot)
        {
            if (!_unacknowledged.TryGetValue(sessionId, out var queue))
            {
                return;
            }
            queue.Remove(fence.FenceId);
            if (!queue.HasPending)
            {
                _unacknowledged.TryRemove(
                    new KeyValuePair<SessionId, PendingHostFenceQueue>(sessionId, queue));
            }
        }
    }

    private bool TryQueueRuntimePendingHostFence(
        SessionId sessionId,
        ILiveSessionHostState host,
        PendingHostFence fence)
    {
        if (host.TryQueuePendingHostFence(fence))
        {
            return true;
        }

        RecordSendFailure(ChannelByteSendQueueWriteOutcome.Full);
        _logger.LogError(
            "Runtime pending host fence capacity reached for session {SessionId}; disconnecting host fail-closed before dropping {FenceType}",
            sessionId,
            fence.GetType().Name);
        _disconnectHost(sessionId, CloseReason.ServerError);
        return false;
    }

    private void QueuePendingHostFenceAfterFailedSend(
        SessionId sessionId,
        LiveSessionPorts ports,
        PendingHostFence fence,
        ChannelByteSendQueueWriteOutcome outcome)
    {
        RecordSendFailure(outcome);
        if (!TryQueueRuntimePendingHostFence(sessionId, ports.Host, fence))
        {
            return;
        }
        _logger.LogInformation(
            "Queued pending host fence {FenceType} for session {SessionId} after send outcome {Outcome}",
            fence.GetType().Name,
            sessionId,
            outcome);
    }

    private bool HasPreHostPendingHostFences(SessionId sessionId)
    {
        return _preHostFenceQueues.TryGetValue(sessionId, out var queue) && queue.HasPending;
    }

    private void QueuePreHostPendingHostFence(
        SessionId sessionId,
        PendingHostFence fence,
        ChannelByteSendQueueWriteOutcome outcome)
    {
        RecordSendFailure(outcome);
        var queue = _preHostFenceQueues.GetOrAdd(
            sessionId,
            static _ => new PendingHostFenceQueue(PreHostPendingFenceCapacity));
        if (!queue.TryQueue(fence))
        {
            _logger.LogError(
                "Pre-host pending host fence capacity reached for session {SessionId}; disconnecting host fail-closed before dropping {FenceType}",
                sessionId,
                fence.GetType().Name);
            _disconnectHost(sessionId, CloseReason.ServerError);
            return;
        }

        _logger.LogInformation(
            "Queued pre-host pending host fence {FenceType} for session {SessionId} after send outcome {Outcome}",
            fence.GetType().Name,
            sessionId,
            outcome);
    }

    private void DrainPreHostFencesToHost(SessionId sessionId, ILiveSessionHostState host)
    {
        if (!_preHostFenceQueues.TryGetValue(sessionId, out var queue))
        {
            return;
        }

        while (true)
        {
            var fence = queue.Peek();
            if (fence is null)
            {
                _preHostFenceQueues.TryRemove(sessionId, out _);
                return;
            }

            if (!host.TryQueuePendingHostFence(fence))
            {
                _logger.LogError(
                    "Runtime pending host fence capacity reached for session {SessionId}; disconnecting host fail-closed before dropping {FenceType}",
                    sessionId,
                    fence.GetType().Name);
                _disconnectHost(sessionId, CloseReason.ServerError);
                return;
            }
            if (!queue.Remove(fence))
            {
                _logger.LogWarning(
                    "Pre-host pending host fence {FenceType} for session {SessionId} changed during drain; stopping drain",
                    fence.GetType().Name,
                    sessionId);
                return;
            }
        }
    }

    private void RecordSendFailure(ChannelByteSendQueueWriteOutcome outcome)
    {
        if (outcome == ChannelByteSendQueueWriteOutcome.Full)
        {
            _metrics.RecordQueueOverflow(QueueOverflowLane.Host);
        }
    }

    private ChannelByteSendQueueWriteOutcome SendFence(SessionId sessionId, PendingHostFence fence)
    {
        return _sendToHost(sessionId, BuildFenceMessage(sessionId, fence));
    }

    private static byte[] BuildFenceMessage(SessionId sessionId, PendingHostFence fence)
    {
        var sessionIdText = sessionId.Value.ToString();
        return fence switch
        {
            PendingHostFence.KeyDistributionRequested =>
                RelayOutbound.Encode(new HostKeyDistributionRequestedMessage(
                    sessionIdText,
                    fence.FenceId)),
            PendingHostFence.ParticipantChanged participantChanged =>
                RelayOutbound.Encode(new HostParticipantChangedMessage(
                    sessionIdText,
                    fence.FenceId,
                    participantChanged.ParticipantUserId.Value.ToString(),
                    participantChanged.ParticipantCount,
                    participantChanged.Action)),
            PendingHostFence.AccessRevoked accessRevoked =>
                RelayOutbound.Encode(new HostAccessRevokedMessage(
                    sessionIdText,
                    fence.FenceId,
                    accessRevoked.RevokedUserId.Value.ToString())),
            _ => throw new ArgumentOutOfRangeException(nameof(fence), fence, "Unknown host fence."),
        };
    }

    private void ReleaseLock(SessionId sessionId, FenceLockEntry entry)
    {
        var remove = false;
        lock (entry.StateLock)
        {
            entry.ReferenceCount--;
            if (entry.ReferenceCount == 0)
            {
                entry.Retired = true;
                remove = true;
            }
        }

        if (remove)
        {
            _hostFenceLocks.TryRemove(
                new KeyValuePair<SessionId, FenceLockEntry>(sessionId, entry));
        }
    }

    internal sealed class FenceLockEntry
    {
        public Lock StateLock { get; } = new();
        public Lock SyncRoot { get; } = new();
        public int ReferenceCount;
        public bool Retired;
    }

    public sealed class FenceLockLease : IDisposable
    {
        private HostFenceCoordinator? _owner;
        private readonly SessionId _sessionId;
        private readonly FenceLockEntry _entry;

        internal FenceLockLease(
            HostFenceCoordinator owner,
            SessionId sessionId,
            FenceLockEntry entry)
        {
            _owner = owner;
            _sessionId = sessionId;
            _entry = entry;
        }

        public Lock SyncRoot => _entry.SyncRoot;

        public void Dispose() =>
            Interlocked.Exchange(ref _owner, null)
                ?.ReleaseLock(_sessionId, _entry);
    }
}
