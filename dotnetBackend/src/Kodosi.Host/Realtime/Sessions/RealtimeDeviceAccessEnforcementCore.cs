using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class RealtimeDeviceAccessEnforcementCore(
    IConnectionRegistry connections,
    SessionBroadcaster sessions,
    UserEventBroadcaster userEvents)
{
    public IDisposable FenceNewConnections(
        UserId userId,
        IReadOnlyCollection<string> deviceIds) =>
        new CompositeFence(
            connections.FenceDeviceRegistrations(userId, deviceIds),
            userEvents.FenceRegistrations(userId, deviceIds));

    public IReadOnlyList<SessionId> GetAffectedSessionIds(
        UserId userId,
        IReadOnlyCollection<string> deviceIds,
        IReadOnlyCollection<SessionId> sessionsWithRevokedKeys) =>
        deviceIds
            .Order(StringComparer.Ordinal)
            .SelectMany(deviceId => connections.GetConnectionsForDevice(userId, deviceId))
            .Select(connection => connection.SessionId)
            .Concat(sessionsWithRevokedKeys)
            .Distinct()
            .OrderBy(sessionId => sessionId.Value)
            .ToList();

    public void EnforceCommitted(
        UserId userId,
        IReadOnlyCollection<string> deviceIds,
        IReadOnlyCollection<SessionId> sessionsWithRevokedKeys,
        CloseReason closeReason)
    {
        var sessionsToRotate = sessionsWithRevokedKeys.ToHashSet();
        foreach (var deviceId in deviceIds)
        {
            userEvents.DisconnectDevice(userId, deviceId, closeReason);
            foreach (var connection in connections.CancelConnectionsForDevice(userId, deviceId))
            {
                if (connection.Kind == SessionConnectionKind.Host)
                {
                    sessions.ForceDisconnectHost(connection.SessionId, closeReason);
                    continue;
                }

                sessions.DisconnectParticipant(
                    connection.SessionId,
                    connection.ConnectionId);
                sessionsToRotate.Add(connection.SessionId);
            }
        }

        foreach (var sessionId in sessionsToRotate)
        {
            sessions.NotifyHostAccessRevoked(sessionId, userId);
        }
    }

    private sealed class CompositeFence(
        IDisposable sessionFence,
        IDisposable userEventFence) : IDisposable
    {
        private IDisposable? _sessionFence = sessionFence;
        private IDisposable? _userEventFence = userEventFence;

        public void Dispose()
        {
            Interlocked.Exchange(ref _userEventFence, null)?.Dispose();
            Interlocked.Exchange(ref _sessionFence, null)?.Dispose();
        }
    }
}
