using Kodosi.Domain;

namespace Kodosi.Application;

public interface IAccessOverrideRepository
{
    Task<SessionAccessOverride?> GetAsync(
        SessionId sessionId,
        UserId actorUserId,
        CancellationToken ct = default);

    Task<SessionAccessOverride?> GetActiveAsync(
        SessionId sessionId,
        UserId actorUserId,
        CancellationToken ct = default);

    Task<IReadOnlyList<SessionAccessOverride>> GetActiveForActorAsync(
        UserId actorUserId,
        IReadOnlyList<SessionId> sessionIds,
        CancellationToken ct = default);

    Task<IReadOnlyList<SessionAccessOverride>> GetActiveBySessionAsync(
        SessionId sessionId,
        CancellationToken ct = default);

    Task<IReadOnlyList<SessionAccessOverride>> GetUnrevokedBySessionAsync(
        SessionId sessionId,
        CancellationToken ct = default);

    Task AddAsync(SessionAccessOverride accessOverride, CancellationToken ct = default);
    Task UpdateAsync(SessionAccessOverride accessOverride, CancellationToken ct = default);

    Task UpdateRangeAsync(
        IReadOnlyCollection<SessionAccessOverride> accessOverrides,
        CancellationToken ct = default);

    Task<IReadOnlyList<UserId>> GetActiveRelatedUserIdsAsync(
        UserId userId,
        CancellationToken ct = default);

    Task<bool> HasActiveRelationshipAsync(UserId userA, UserId userB, CancellationToken ct = default);

    Task<IReadOnlyList<SessionAccessOverride>> GetExpiredUnrevokedAsync(
        DateTimeOffset now,
        int limit,
        CancellationToken ct = default);

    Task<bool> TryRevokeExpiredAsync(
        SessionId sessionId,
        UserId actorUserId,
        DateTimeOffset observedExpiry,
        DateTimeOffset revokedAt,
        CancellationToken ct = default);
}
