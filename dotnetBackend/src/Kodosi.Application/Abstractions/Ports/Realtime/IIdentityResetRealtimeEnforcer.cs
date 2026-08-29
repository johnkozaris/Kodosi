using Kodosi.Domain;

namespace Kodosi.Application;

public interface IIdentityResetRealtimeEnforcer
{
    IDisposable FenceNewConnections(
        UserId userId,
        IReadOnlyCollection<string> removedDeviceIds);

    IReadOnlyList<SessionId> GetAffectedSessionIds(
        UserId userId,
        IReadOnlyCollection<string> removedDeviceIds,
        IReadOnlyCollection<SessionId> sessionsWithRevokedKeys);

    Task EnforceCommittedAsync(
        UserId userId,
        IReadOnlyCollection<string> removedDeviceIds,
        IReadOnlyCollection<SessionId> sessionsWithRevokedKeys,
        CancellationToken ct = default);
}
