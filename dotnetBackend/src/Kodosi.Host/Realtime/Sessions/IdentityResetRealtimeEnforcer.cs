using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class IdentityResetRealtimeEnforcer : IIdentityResetRealtimeEnforcer
{
    private readonly RealtimeDeviceAccessEnforcementCore _enforcement;

    public IdentityResetRealtimeEnforcer(
        RealtimeDeviceAccessEnforcementCore enforcement)
    {
        _enforcement = enforcement;
    }

    public IDisposable FenceNewConnections(
        UserId userId,
        IReadOnlyCollection<string> removedDeviceIds) =>
        _enforcement.FenceNewConnections(userId, removedDeviceIds);

    public IReadOnlyList<SessionId> GetAffectedSessionIds(
        UserId userId,
        IReadOnlyCollection<string> removedDeviceIds,
        IReadOnlyCollection<SessionId> sessionsWithRevokedKeys) =>
        _enforcement.GetAffectedSessionIds(
            userId,
            removedDeviceIds,
            sessionsWithRevokedKeys);

    public Task EnforceCommittedAsync(
        UserId userId,
        IReadOnlyCollection<string> removedDeviceIds,
        IReadOnlyCollection<SessionId> sessionsWithRevokedKeys,
        CancellationToken ct = default)
    {
        ct.ThrowIfCancellationRequested();
        _enforcement.EnforceCommitted(
            userId,
            removedDeviceIds,
            sessionsWithRevokedKeys,
            CloseReason.OwnerIdentityReset);
        return Task.CompletedTask;
    }
}
