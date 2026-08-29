using Kodosi.Domain;

namespace Kodosi.Application;

public enum SessionConnectionKind
{
    Host,
    OwnerParticipant,
    SharedParticipant,
}

public sealed record DeviceConnectionRegistration(
    string ConnectionId,
    SessionId SessionId,
    SessionConnectionKind Kind);

public interface IConnectionRegistry
{
    int ActiveConnectionCount { get; }
    void RegisterHost(
        string connectionId,
        UserId userId,
        string deviceId,
        SessionId sessionId,
        CancellationTokenSource? cancellationTarget = null);
    void RegisterOwnerParticipant(
        string connectionId,
        UserId userId,
        string deviceId,
        SessionId sessionId,
        CancellationTokenSource? cancellationTarget = null);
    void RegisterSharedParticipant(
        string connectionId,
        UserId userId,
        string deviceId,
        SessionId sessionId,
        CancellationTokenSource? cancellationTarget = null);
    bool IsCurrentConnection(
        string connectionId,
        UserId userId,
        string deviceId,
        SessionId sessionId,
        SessionConnectionKind kind);
    void Remove(string connectionId);
    IReadOnlyList<string> GetSharedParticipantConnectionIds(SessionId sessionId, UserId userId);
    IReadOnlyList<(string ConnectionId, UserId UserId)> GetActiveParticipants(SessionId sessionId);
    IReadOnlyList<(string ConnectionId, UserId UserId)> GetActiveSharedParticipants(SessionId sessionId);
    IReadOnlyList<DeviceConnectionRegistration> GetConnectionsForDevice(
        UserId userId,
        string deviceId);
    IDisposable FenceDeviceRegistrations(
        UserId userId,
        IReadOnlyCollection<string> deviceIds);
    IReadOnlyList<DeviceConnectionRegistration> CancelConnectionsForDevice(
        UserId userId,
        string deviceId);
}
