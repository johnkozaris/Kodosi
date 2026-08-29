using System.Collections.Concurrent;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed partial class SessionBroadcaster
{
    private readonly ConcurrentDictionary<SessionId, SessionSendQueues> _sessions = new();
    private readonly ILiveSessionStateDirectory _runtimes;
    private readonly OperationalMetrics _metrics;
    private readonly ILogger<SessionBroadcaster> _logger;
    private readonly HostFenceCoordinator _fences;
    public string RelayEpoch { get; } = Guid.NewGuid().ToString("N");

    public SessionBroadcaster(
        ILiveSessionStateDirectory runtimes,
        OperationalMetrics metrics,
        ILoggerFactory loggerFactory)
    {
        _runtimes = runtimes;
        _metrics = metrics;
        _logger = loggerFactory.CreateLogger<SessionBroadcaster>();
        _fences = new HostFenceCoordinator(
            runtimes,
            metrics,
            loggerFactory.CreateLogger<HostFenceCoordinator>(),
            SendToHost,
            RejectHostAdmission);
    }

    public SessionSendQueues GetOrCreateSession(SessionId sessionId)
    {
        return _sessions.GetOrAdd(sessionId, _ => new SessionSendQueues());
    }

    public SessionSendQueues? TryGetSession(SessionId sessionId)
    {
        return _sessions.TryGetValue(sessionId, out var queues) ? queues : null;
    }

    public IReadOnlyList<RelayClientSendQueue> GetAllDownstreamQueues()
    {
        return _sessions.Values
            .SelectMany(EnumerateDownstreamQueues)
            .Select(entry => entry.Value)
            .ToList();
    }

    public int CompleteAllHostQueues(CloseReason reason)
    {
        var completed = 0;
        foreach (var sendQueues in _sessions.Values)
        {
            completed += sendQueues.CompleteHostQueues(reason);
        }

        return completed;
    }

    public bool RemoveSessionIfSame(
        SessionId sessionId,
        SessionSendQueues? sendQueues,
        CloseReason reason = CloseReason.ClosingNormal)
    {
        using var fenceLock = _fences.AcquireLock(sessionId);
        lock (fenceLock.SyncRoot)
        {
            if (sendQueues is null)
            {
                return false;
            }

            if (!((ICollection<KeyValuePair<SessionId, SessionSendQueues>>)_sessions)
                    .Remove(new KeyValuePair<SessionId, SessionSendQueues>(sessionId, sendQueues)))
            {
                return false;
            }

            sendQueues.CloseAll(reason, ToParticipantCompletionCause(reason), reason);
            _fences.RemovePreHostQueue(sessionId);
        }

        return true;
    }

    private static QueueCompletionCause ToParticipantCompletionCause(CloseReason closeReason) =>
        closeReason switch
        {
            CloseReason.ServerRestarting => QueueCompletionCause.Drain,
            CloseReason.ParticipantTimeout => QueueCompletionCause.Timeout,
            CloseReason.ClosingNormal => QueueCompletionCause.Normal,
            _ => QueueCompletionCause.SessionEnd,
        };

    private void BroadcastControlToDownstreamClients(SessionId sessionId, WireMessage message)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues))
        {
            return;
        }

        foreach (var (connectionId, downstreamQueue) in EnumerateDownstreamQueues(sendQueues))
        {
            TryEnqueueControl(sessionId, connectionId, downstreamQueue, message);
        }
    }

    private bool BroadcastKeyRotationToDownstreamClientsIfSame(
        SessionId sessionId,
        SessionSendQueues expectedQueues,
        WireMessage message)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues)
            || !ReferenceEquals(sendQueues, expectedQueues))
        {
            return false;
        }

        foreach (var (connectionId, downstreamQueue) in
            EnumerateDownstreamQueues(sendQueues))
        {
            var outcome = downstreamQueue.TryEnqueueKeyRotation(message);
            HandleControlEnqueueOutcome(sessionId, connectionId, outcome);
        }
        return true;
    }

    private bool TryEnqueueControl(
        SessionId sessionId,
        string connectionId,
        RelayClientSendQueue downstreamQueue,
        WireMessage message)
    {
        var outcome = downstreamQueue.TryEnqueueControl(message);
        return HandleControlEnqueueOutcome(sessionId, connectionId, outcome);
    }

    private bool HandleControlEnqueueOutcome(
        SessionId sessionId,
        string connectionId,
        RelayClientSendQueueWriteOutcome outcome)
    {
        if (outcome == RelayClientSendQueueWriteOutcome.Enqueued)
        {
            return true;
        }

        if (outcome == RelayClientSendQueueWriteOutcome.Overflow)
        {
            _metrics.RecordControlQueueOverflowDisconnect();
            _logger.LogWarning(
                "Downstream client {ConnectionId} overflowed control queue for session {SessionId}; disconnecting",
                connectionId,
                sessionId);
        }

        return false;
    }

    private static IEnumerable<KeyValuePair<string, RelayClientSendQueue>> EnumerateDownstreamQueues(
        SessionSendQueues queues)
    {
        foreach (var entry in queues.GetParticipantQueueSnapshot())
        {
            yield return entry;
        }
    }

    private IReadOnlyList<WireMessage>? TryBuildResyncReplay(SessionId sessionId)
    {
        var ports = _runtimes.TryGet(sessionId);
        if (ports is null)
        {
            return null;
        }

        var replayState = ports.Stream.GetReplayState();
        var messages = new List<WireMessage>();
        if (replayState.TerminalCheckpoint is { } checkpoint)
        {
            messages.Add(WireMessage.EncryptedBinary(checkpoint.EncryptedBlob));
            foreach (var rawBatch in replayState.TerminalRawBatches)
            {
                messages.Add(WireMessage.EncryptedBinary(rawBatch.EncryptedBlob));
            }
        }

        if (replayState.TerminalPresentation is { } presentation)
        {
            messages.Add(WireMessage.EncryptedBinary(presentation.EncryptedBlob));
        }

        if (messages.Count == 0 && replayState.KeyRotation is { } keyRotation)
        {
            messages.Add(WireMessage.Json(keyRotation));
        }

        return messages.Count == 0 ? null : messages;
    }
}
