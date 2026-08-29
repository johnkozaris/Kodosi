using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Realtime;



public sealed class ConnectionRegistry : IConnectionRegistry
{
    private readonly Lock _lock = new();
    private readonly Dictionary<string, ConnectionRegistration> _connections = new(StringComparer.Ordinal);
    private readonly Dictionary<SessionId, Dictionary<string, ConnectionRegistration>> _connectionsBySession = [];
    private readonly Dictionary<(SessionId SessionId, UserId UserId), HashSet<string>>
        _sharedConnectionsBySessionAndUser = [];
    private readonly Dictionary<(UserId UserId, string DeviceId), int>
        _deviceRegistrationFences = [];

    public int ActiveConnectionCount
    {
        get
        {
            lock (_lock)
            {
                return _connections.Count;
            }
        }
    }

    public void RegisterHost(
        string connectionId,
        UserId userId,
        string deviceId,
        SessionId sessionId,
        CancellationTokenSource? cancellationTarget = null)
    {
        RegisterConnection(
            connectionId,
            userId,
            deviceId,
            sessionId,
            SessionConnectionKind.Host,
            cancellationTarget);
    }

    public void RegisterOwnerParticipant(
        string connectionId,
        UserId userId,
        string deviceId,
        SessionId sessionId,
        CancellationTokenSource? cancellationTarget = null)
    {
        RegisterConnection(
            connectionId,
            userId,
            deviceId,
            sessionId,
            SessionConnectionKind.OwnerParticipant,
            cancellationTarget);
    }

    public void RegisterSharedParticipant(
        string connectionId,
        UserId userId,
        string deviceId,
        SessionId sessionId,
        CancellationTokenSource? cancellationTarget = null)
    {
        RegisterConnection(
            connectionId,
            userId,
            deviceId,
            sessionId,
            SessionConnectionKind.SharedParticipant,
            cancellationTarget);
    }

    public bool IsCurrentConnection(
        string connectionId,
        UserId userId,
        string deviceId,
        SessionId sessionId,
        SessionConnectionKind kind)
    {
        lock (_lock)
        {
            return _connections.TryGetValue(connectionId, out var registration)
                && registration.UserId == userId
                && registration.SessionId == sessionId
                && registration.Kind == kind
                && string.Equals(
                    registration.DeviceId,
                    deviceId,
                    StringComparison.Ordinal);
        }
    }

    public void Remove(string connectionId)
    {
        lock (_lock)
        {
            if (!_connections.Remove(connectionId, out var existing))
            {
                return;
            }

            RemoveSessionIndex(connectionId, existing);
            RemoveSharedParticipantIndex(connectionId, existing);
        }
    }

    public IReadOnlyList<string> GetSharedParticipantConnectionIds(SessionId sessionId, UserId userId)
    {
        lock (_lock)
        {
            return _sharedConnectionsBySessionAndUser.TryGetValue((sessionId, userId), out var connectionIds)
                ? [.. connectionIds]
                : [];
        }
    }

    public IReadOnlyList<(string ConnectionId, UserId UserId)> GetActiveParticipants(SessionId sessionId)
    {
        lock (_lock)
        {
            if (!_connectionsBySession.TryGetValue(sessionId, out var sessionConnections))
            {
                return [];
            }

            return sessionConnections
                .Where(kvp => kvp.Value.Kind is SessionConnectionKind.SharedParticipant or SessionConnectionKind.OwnerParticipant)
                .Select(kvp => (kvp.Key, kvp.Value.UserId))
                .ToList();
        }
    }

    public IReadOnlyList<(string ConnectionId, UserId UserId)> GetActiveSharedParticipants(SessionId sessionId)
    {
        lock (_lock)
        {
            if (!_connectionsBySession.TryGetValue(sessionId, out var sessionConnections))
            {
                return [];
            }

            return sessionConnections
                .Where(kvp => kvp.Value.Kind == SessionConnectionKind.SharedParticipant)
                .Select(kvp => (kvp.Key, kvp.Value.UserId))
                .ToList();
        }
    }

    public IReadOnlyList<DeviceConnectionRegistration> GetConnectionsForDevice(
        UserId userId,
        string deviceId)
    {
        lock (_lock)
        {
            return GetConnectionsForDeviceLocked(userId, deviceId)
                .Select(ToDeviceRegistration)
                .ToList();
        }
    }

    public IDisposable FenceDeviceRegistrations(
        UserId userId,
        IReadOnlyCollection<string> deviceIds)
    {
        var keys = deviceIds
            .Distinct(StringComparer.Ordinal)
            .Order(StringComparer.Ordinal)
            .Select(deviceId => (userId, deviceId))
            .ToList();
        lock (_lock)
        {
            foreach (var key in keys)
            {
                _deviceRegistrationFences[key] =
                    _deviceRegistrationFences.GetValueOrDefault(key) + 1;
            }
        }

        return new DeviceRegistrationFence(this, keys);
    }

    public IReadOnlyList<DeviceConnectionRegistration> CancelConnectionsForDevice(
        UserId userId,
        string deviceId)
    {
        List<KeyValuePair<string, ConnectionRegistration>> registrations;
        lock (_lock)
        {
            registrations = GetConnectionsForDeviceLocked(userId, deviceId);
        }

        foreach (var registration in registrations)
        {
            registration.Value.Cancel();
        }

        return registrations
            .Select(ToDeviceRegistration)
            .ToList();
    }

    private void RegisterConnection(
        string connectionId,
        UserId userId,
        string deviceId,
        SessionId sessionId,
        SessionConnectionKind kind,
        CancellationTokenSource? cancellationTarget)
    {
        var registration = new ConnectionRegistration(
            userId,
            deviceId,
            sessionId,
            kind,
            cancellationTarget);
        var fenced = false;
        lock (_lock)
        {
            if (_connections.TryGetValue(connectionId, out var existing))
            {
                RemoveSessionIndex(connectionId, existing);
                RemoveSharedParticipantIndex(connectionId, existing);
            }

            _connections[connectionId] = registration;

            if (!_connectionsBySession.TryGetValue(sessionId, out var sessionConnections))
            {
                sessionConnections = new Dictionary<string, ConnectionRegistration>(StringComparer.Ordinal);
                _connectionsBySession[sessionId] = sessionConnections;
            }

            sessionConnections[connectionId] = registration;

            if (kind == SessionConnectionKind.SharedParticipant)
            {
                var key = (sessionId, userId);
                if (!_sharedConnectionsBySessionAndUser.TryGetValue(key, out var sharedConnectionIds))
                {
                    sharedConnectionIds = new HashSet<string>(StringComparer.Ordinal);
                    _sharedConnectionsBySessionAndUser[key] = sharedConnectionIds;
                }

                sharedConnectionIds.Add(connectionId);
            }

            fenced = _deviceRegistrationFences.ContainsKey((userId, deviceId));
        }

        if (fenced)
        {
            registration.Cancel();
        }
    }

    private void ReleaseDeviceRegistrationFence(
        IReadOnlyCollection<(UserId UserId, string DeviceId)> keys)
    {
        lock (_lock)
        {
            foreach (var key in keys)
            {
                var remaining = _deviceRegistrationFences[key] - 1;
                if (remaining == 0)
                {
                    _deviceRegistrationFences.Remove(key);
                }
                else
                {
                    _deviceRegistrationFences[key] = remaining;
                }
            }
        }
    }

    private List<KeyValuePair<string, ConnectionRegistration>>
        GetConnectionsForDeviceLocked(
            UserId userId,
            string deviceId) =>
        _connections
            .Where(pair => pair.Value.UserId == userId
                && string.Equals(
                    pair.Value.DeviceId,
                    deviceId,
                    StringComparison.Ordinal))
            .ToList();

    private static DeviceConnectionRegistration ToDeviceRegistration(
        KeyValuePair<string, ConnectionRegistration> pair) =>
        new(
            pair.Key,
            pair.Value.SessionId,
            pair.Value.Kind);

    private void RemoveSessionIndex(string connectionId, ConnectionRegistration registration)
    {
        if (!_connectionsBySession.TryGetValue(registration.SessionId, out var sessionConnections))
        {
            return;
        }

        sessionConnections.Remove(connectionId);
        if (sessionConnections.Count == 0)
        {
            _connectionsBySession.Remove(registration.SessionId);
        }
    }

    private void RemoveSharedParticipantIndex(string connectionId, ConnectionRegistration registration)
    {
        if (registration.Kind != SessionConnectionKind.SharedParticipant)
        {
            return;
        }

        var key = (registration.SessionId, registration.UserId);
        if (!_sharedConnectionsBySessionAndUser.TryGetValue(key, out var connectionIds))
        {
            return;
        }

        connectionIds.Remove(connectionId);
        if (connectionIds.Count == 0)
        {
            _sharedConnectionsBySessionAndUser.Remove(key);
        }
    }

    private sealed record ConnectionRegistration(
        UserId UserId,
        string DeviceId,
        SessionId SessionId,
        SessionConnectionKind Kind,
        CancellationTokenSource? CancellationTarget)
    {
        public void Cancel()
        {
            try
            {
                CancellationTarget?.Cancel();
            }
            catch (ObjectDisposedException)
            {

            }
        }
    }

    private sealed class DeviceRegistrationFence(
        ConnectionRegistry owner,
        IReadOnlyCollection<(UserId UserId, string DeviceId)> keys)
        : IDisposable
    {
        private ConnectionRegistry? _owner = owner;

        public void Dispose() =>
            Interlocked.Exchange(ref _owner, null)
                ?.ReleaseDeviceRegistrationFence(keys);
    }
}
