using Kodosi.Domain;

namespace Kodosi.Application;

public interface IIdentityResetDurabilityCoordinator
{
    Task<IdentityResetCommitReconciliation> ReconcileCommitAsync(
        Guid resetId,
        UserId userId,
        IReadOnlyCollection<string> expectedRemovedDeviceIds,
        CancellationToken ct = default);

    Task<IReadOnlyList<IdentityResetEnforcementWork>> GetPendingEnforcementAsync(
        int limit,
        CancellationToken ct = default);

    Task<bool> ExecuteIfCurrentAsync(
        IdentityResetEnforcementWork work,
        Func<CancellationToken, Task> enforce,
        CancellationToken ct = default);

    Task CompleteEnforcementAsync(
        Guid resetId,
        CancellationToken ct = default);
}

public interface IIdentityResetSessionResolver
{
    Task<IReadOnlyList<IdentityResetSessionTarget>> GetCurrentTargetsAsync(
        IReadOnlyCollection<IdentityResetSessionTarget> targets,
        CancellationToken ct = default);
}

public enum IdentityResetCommitState
{
    Committed,
    NotCommitted,
    Inconsistent,
}

public sealed record IdentityResetCommitReconciliation(
    IdentityResetCommitState State,
    IdentityResetEnforcementWork? Work = null,
    string? Detail = null);

public sealed record IdentityResetEndedSession(
    SessionDiscoveryTarget Target,
    DateTimeOffset StartedAt);

public sealed record IdentityResetEnforcementWork(
    Guid ResetId,
    UserId UserId,
    long IdentityRevision,
    IReadOnlyList<string> RemovedDeviceIds,
    IReadOnlyList<SessionId> EndedSessionIds,
    IReadOnlyList<SessionId> SessionsWithRevokedKeys)
{
    public IReadOnlyList<IdentityResetSessionTarget> SessionTargets { get; init; } = [];
    public IReadOnlyList<IdentityResetEndedSession> EndedSessions { get; init; } = [];
}
