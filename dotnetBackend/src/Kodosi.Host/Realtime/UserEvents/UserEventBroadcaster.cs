using System.Collections.Concurrent;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed class UserEventBroadcaster(
    OperationalMetrics metrics,
    ILogger<UserEventBroadcaster> logger)
{
    private readonly ConcurrentDictionary<string, UserEventConnection> _connections = new();
    private readonly Lock _registrationLock = new();
    private readonly Dictionary<(UserId UserId, string DeviceId), int>
        _registrationFences = [];
    private readonly OperationalMetrics _metrics = metrics;
    private readonly ILogger<UserEventBroadcaster> _logger = logger;

    public int ActiveConnectionCount => _connections.Count;

    public UserEventRegistration RegisterPrimed(
        string connectionId,
        UserId userId,
        string deviceId)
    {
        var queue = new ChannelByteSendQueue();
        var connection = new UserEventConnection(
            userId,
            deviceId,
            queue,
            priming: true);
        lock (_registrationLock)
        {
            if (_registrationFences.ContainsKey((userId, deviceId)))
            {
                queue.Complete(CloseReason.AccessRevoked, discardPending: true);
                return new UserEventRegistration(connection);
            }

            if (_connections.TryGetValue(connectionId, out var existing))
            {
                existing.Queue.Complete();
            }

            _connections[connectionId] = connection;
        }
        return new UserEventRegistration(connection);
    }

    public IDisposable FenceRegistrations(
        UserId userId,
        IReadOnlyCollection<string> deviceIds)
    {
        var keys = deviceIds
            .Distinct(StringComparer.Ordinal)
            .Order(StringComparer.Ordinal)
            .Select(deviceId => (userId, deviceId))
            .ToList();
        lock (_registrationLock)
        {
            foreach (var key in keys)
            {
                _registrationFences[key] =
                    _registrationFences.GetValueOrDefault(key) + 1;
            }
        }

        return new RegistrationFence(this, keys);
    }

    public void Remove(string connectionId)
    {
        if (_connections.TryRemove(connectionId, out var connection))
        {
            connection.Queue.Complete();
        }
    }

    public int CompleteAll(CloseReason reason)
    {
        var completed = 0;
        foreach (var connection in _connections.Values)
        {
            connection.Queue.Complete(reason);
            completed++;
        }
        return completed;
    }

    public int DisconnectDevice(
        UserId userId,
        string deviceId,
        CloseReason closeReason = CloseReason.AccessRevoked)
    {
        var disconnected = 0;
        foreach (var connection in _connections.Values)
        {
            if (connection.UserId != userId
                || !string.Equals(connection.DeviceId, deviceId, StringComparison.Ordinal))
            {
                continue;
            }

            connection.Queue.Complete(closeReason, discardPending: true);
            disconnected++;
        }
        return disconnected;
    }

    public void PublishInvalidation(
        DiscoveryAudience audience,
        IReadOnlySet<DiscoverySurface> surfaces,
        RoomId? roomId = null)
    {
        if (surfaces.Count == 0)
        {
            return;
        }

        if (audience.UserIds.Count == 0)
        {
            return;
        }

        var orderedSurfaces = surfaces
            .Distinct()
            .OrderBy(surface => surface)
            .ToArray();
        var payload = RelayOutbound.Encode(new DiscoveryInvalidatedMessage(
            orderedSurfaces,
            roomId?.Value.ToString()));

        foreach (var (connectionId, connection) in _connections)
        {
            if (!audience.UserIds.Contains(connection.UserId))
            {
                continue;
            }

            TryNotifyConnection(connectionId, connection, payload);
        }
    }

    public void PublishDeviceListChanged(
        DiscoveryAudience audience,
        UserId targetUserId,
        long generation)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(generation);
        if (audience.UserIds.Count == 0)
        {
            return;
        }

        var payload = RelayOutbound.Encode(
            new UserDeviceListChangedMessage(targetUserId.Value.ToString(), generation));

        foreach (var (connectionId, connection) in _connections)
        {
            if (!audience.UserIds.Contains(connection.UserId))
            {
                continue;
            }

            TryNotifyConnection(connectionId, connection, payload);
        }
    }

    public void PublishIdentityLifecycleChanged(
        DiscoveryAudience audience,
        UserId targetUserId,
        long identityRevision,
        Guid? incarnationId,
        long generation)
    {
        if (audience.UserIds.Count == 0)
        {
            return;
        }
        var state = incarnationId.HasValue ? "enrolled" : "withdrawn";
        var payload = RelayOutbound.Encode(new UserIdentityLifecycleChangedMessage(
            targetUserId.Value.ToString(),
            identityRevision,
            incarnationId,
            state,
            generation));
        foreach (var (connectionId, connection) in _connections)
        {
            if (audience.UserIds.Contains(connection.UserId))
            {
                TryNotifyConnection(connectionId, connection, payload);
            }
        }
    }

    public void PublishDeviceLinkRequested(
        UserId userId,
        string userCode,
        string deviceLabel,
        DateTimeOffset expiresAt)
    {
        var payload = RelayOutbound.Encode(new UserDeviceLinkRequestedMessage(userCode, deviceLabel, expiresAt));

        NotifyUser(userId, payload);
    }

    public void PublishDeviceLinkResolved(UserId userId, string userCode, string outcome)
    {
        var payload = RelayOutbound.Encode(new UserDeviceLinkResolvedMessage(userCode, outcome));

        NotifyUser(userId, payload);
    }

    private void NotifyUser(UserId userId, byte[] payload)
    {
        foreach (var (connectionId, connection) in _connections)
        {
            if (connection.UserId != userId)
            {
                continue;
            }

            TryNotifyConnection(connectionId, connection, payload);
        }
    }

    private void TryNotifyConnection(
        string connectionId,
        UserEventConnection connection,
        byte[] payload)
    {
        var outcome = connection.TryEnqueueLive(payload);
        if (outcome == ChannelByteSendQueueWriteOutcome.Enqueued)
        {
            return;
        }

        if (outcome == ChannelByteSendQueueWriteOutcome.Full)
        {
            _metrics.RecordQueueOverflow(QueueOverflowLane.UserEvents);
            _logger.LogWarning(
                "User event queue overflow for connection {ConnectionId}; forcing reconnect",
                connectionId);
            connection.Queue.Complete(
                CloseReason.ServerError,
                discardPending: true);
            return;
        }

        _logger.LogDebug(
            "Skipped discovery invalidation for closed user event connection {ConnectionId}",
            connectionId);
    }

    internal sealed class UserEventRegistration(UserEventConnection connection)
    {
        private readonly UserEventConnection _connection = connection;

        public ChannelByteSendQueue Queue => _connection.Queue;

        public ChannelByteSendQueueWriteOutcome CompletePriming() =>
            _connection.CompletePriming();
    }

    internal sealed class UserEventConnection(
        UserId userId,
        string deviceId,
        ChannelByteSendQueue queue,
        bool priming)
    {
        private const int MaxDeferredLiveMessages = 256;
        private readonly Lock _lock = new();
        private readonly Queue<byte[]> _deferredLive = [];
        private bool _priming = priming;
        private int _deferredLiveBytes;

        public UserId UserId { get; } = userId;
        public string DeviceId { get; } = deviceId;
        public ChannelByteSendQueue Queue { get; } = queue;

        public ChannelByteSendQueueWriteOutcome TryEnqueueLive(byte[] payload)
        {
            lock (_lock)
            {
                if (!_priming)
                {
                    return Queue.TryEnqueue(payload);
                }

                if (_deferredLive.Count >= MaxDeferredLiveMessages
                    || payload.Length
                        > ChannelByteSendQueue.DefaultMaxBacklogBytes - _deferredLiveBytes)
                {
                    return ChannelByteSendQueueWriteOutcome.Full;
                }
                _deferredLive.Enqueue(payload);
                _deferredLiveBytes += payload.Length;
                return ChannelByteSendQueueWriteOutcome.Enqueued;
            }
        }

        public ChannelByteSendQueueWriteOutcome CompletePriming()
        {
            lock (_lock)
            {
                if (!_priming)
                {
                    return ChannelByteSendQueueWriteOutcome.Closed;
                }

                while (_deferredLive.TryDequeue(out var payload))
                {
                    _deferredLiveBytes -= payload.Length;
                    var outcome = Queue.TryEnqueue(payload);
                    if (outcome != ChannelByteSendQueueWriteOutcome.Enqueued)
                    {
                        _deferredLive.Clear();
                        _deferredLiveBytes = 0;
                        _priming = false;
                        return outcome;
                    }
                }
                _priming = false;
                return ChannelByteSendQueueWriteOutcome.Enqueued;
            }
        }
    }

    private void ReleaseRegistrationFence(
        IReadOnlyCollection<(UserId UserId, string DeviceId)> keys)
    {
        lock (_registrationLock)
        {
            foreach (var key in keys)
            {
                var remaining = _registrationFences[key] - 1;
                if (remaining == 0)
                {
                    _registrationFences.Remove(key);
                }
                else
                {
                    _registrationFences[key] = remaining;
                }
            }
        }
    }

    private sealed class RegistrationFence(
        UserEventBroadcaster owner,
        IReadOnlyCollection<(UserId UserId, string DeviceId)> keys)
        : IDisposable
    {
        private UserEventBroadcaster? _owner = owner;

        public void Dispose() =>
            Interlocked.Exchange(ref _owner, null)
                ?.ReleaseRegistrationFence(keys);
    }
}
