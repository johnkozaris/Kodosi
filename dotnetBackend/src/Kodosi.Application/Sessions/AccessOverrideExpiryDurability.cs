using Kodosi.Domain;

namespace Kodosi.Application;

public interface IAccessOverrideExpiryDurabilityCoordinator
{
    Task<IReadOnlyList<AccessOverrideExpiryEnforcementWork>> GetPendingEnforcementAsync(
        int limit,
        CancellationToken ct = default,
        AccessOverrideExpiryEnforcementCursor? before = null);

    Task<bool> ExecuteIfCurrentAsync(
        AccessOverrideExpiryEnforcementWork work,
        Func<CancellationToken, Task> enforce,
        CancellationToken ct = default);

    Task CompleteEnforcementAsync(
        Guid auditEntryId,
        CancellationToken ct = default);
}

public readonly record struct AccessOverrideExpiryEnforcementCursor(
    DateTimeOffset OccurredAt,
    Guid AuditEntryId);

public sealed record AccessOverrideExpiryEnforcementWork(
    Guid AuditEntryId,
    DateTimeOffset OccurredAt,
    SessionId SessionId,
    UserId GranteeUserId,
    Guid SessionIncarnationId,
    DateTimeOffset SessionStartedAt,
    DateTimeOffset ExpectedExpiresAt,
    DateTimeOffset ExpectedRevokedAt)
{
    public SessionAccessFanoutTarget Target => new(
        SessionId,
        SessionStartedAt,
        SessionIncarnationId);
}
