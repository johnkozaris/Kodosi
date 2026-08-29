using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class AccessOverrideRepository(
    KodosiDbContext context,
    TimeProvider timeProvider) : IAccessOverrideRepository
{
    private readonly KodosiDbContext _context = context;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task<SessionAccessOverride?> GetAsync(
        SessionId sessionId,
        UserId actorUserId,
        CancellationToken ct = default)
        => await _context.SessionAccessOverrides
            .FirstOrDefaultAsync(o =>
                o.SessionId == sessionId
                && o.ActorUserId == actorUserId, ct);

    public async Task<SessionAccessOverride?> GetActiveAsync(
        SessionId sessionId, UserId actorUserId, CancellationToken ct = default)
    {
        var now = _timeProvider.GetUtcNow();
        return await _context.SessionAccessOverrides
            .FirstOrDefaultAsync(o =>
                o.SessionId == sessionId
                && o.ActorUserId == actorUserId
                && o.RevokedAt == null
                && (o.ExpiresAt == null || o.ExpiresAt > now), ct);
    }

    public async Task<IReadOnlyList<SessionAccessOverride>> GetActiveForActorAsync(
        UserId actorUserId,
        IReadOnlyList<SessionId> sessionIds,
        CancellationToken ct = default)
    {
        if (sessionIds.Count == 0)
        {
            return [];
        }

        var now = _timeProvider.GetUtcNow();
        return await _context.SessionAccessOverrides
            .Where(o =>
                o.ActorUserId == actorUserId
                && sessionIds.Contains(o.SessionId)
                && o.RevokedAt == null
                && (o.ExpiresAt == null || o.ExpiresAt > now))
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<SessionAccessOverride>> GetActiveBySessionAsync(
        SessionId sessionId,
        CancellationToken ct = default)
    {
        var now = _timeProvider.GetUtcNow();
        return await _context.SessionAccessOverrides
            .Where(o =>
                o.SessionId == sessionId
                && o.RevokedAt == null
                && (o.ExpiresAt == null || o.ExpiresAt > now))
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<SessionAccessOverride>> GetUnrevokedBySessionAsync(
        SessionId sessionId,
        CancellationToken ct = default) =>
        await _context.SessionAccessOverrides
            .Where(accessOverride =>
                accessOverride.SessionId == sessionId
                && accessOverride.RevokedAt == null)
            .ToListAsync(ct);

    public async Task AddAsync(SessionAccessOverride accessOverride, CancellationToken ct = default)
        => await _context.SessionAccessOverrides.AddAsync(accessOverride, ct);

    public Task UpdateAsync(SessionAccessOverride accessOverride, CancellationToken ct = default)
    {
        _context.SessionAccessOverrides.Update(accessOverride);
        return Task.CompletedTask;
    }

    public Task UpdateRangeAsync(
        IReadOnlyCollection<SessionAccessOverride> accessOverrides,
        CancellationToken ct = default)
    {
        _context.SessionAccessOverrides.UpdateRange(accessOverrides);
        return Task.CompletedTask;
    }

    public async Task<IReadOnlyList<UserId>> GetActiveRelatedUserIdsAsync(
        UserId userId,
        CancellationToken ct = default)
    {
        var now = _timeProvider.GetUtcNow();
        var relatedUserIds = await _context.Database
            .SqlQuery<Guid>($"""
                SELECT DISTINCT
                    CASE
                        WHEN session.owner_user_id = {userId.Value}
                            THEN access_override.actor_user_id
                        ELSE session.owner_user_id
                    END AS "Value"
                FROM session_access_overrides AS access_override
                INNER JOIN sessions AS session
                    ON session.id = access_override.session_id
                WHERE access_override.revoked_at IS NULL
                  AND (access_override.expires_at IS NULL OR access_override.expires_at > {now})
                  AND session.status <> {SessionStatus.Ended.ToString()}
                  AND (
                      session.owner_user_id = {userId.Value}
                      OR access_override.actor_user_id = {userId.Value}
                  )
                  AND session.owner_user_id <> access_override.actor_user_id
                ORDER BY "Value"
                """)
            .ToListAsync(ct);
        return relatedUserIds.Select(UserId.From).ToList();
    }

    public Task<bool> HasActiveRelationshipAsync(UserId userA, UserId userB, CancellationToken ct = default)
    {
        if (userA == userB)
        {
            return Task.FromResult(false);
        }



        var now = _timeProvider.GetUtcNow();
        return _context.SessionAccessOverrides
            .Where(o =>
                o.RevokedAt == null
                && (o.ExpiresAt == null || o.ExpiresAt > now))
            .Join(
                _context.Sessions,
                o => o.SessionId,
                s => s.Id,
                (o, s) => new
                {
                    Owner = s.OwnerUserId,
                    Actor = o.ActorUserId,
                    s.Status,
                })
            .AnyAsync(
                x => x.Status != SessionStatus.Ended
                    && ((x.Owner == userA && x.Actor == userB)
                        || (x.Owner == userB && x.Actor == userA)),
                ct);
    }

    public async Task<IReadOnlyList<SessionAccessOverride>> GetExpiredUnrevokedAsync(
        DateTimeOffset now,
        int limit,
        CancellationToken ct = default)
        => await _context.SessionAccessOverrides
            .Where(accessOverride =>
                accessOverride.RevokedAt == null
                && accessOverride.ExpiresAt != null
                && accessOverride.ExpiresAt <= now)
            .OrderBy(accessOverride => accessOverride.ExpiresAt)
            .Take(Math.Clamp(limit, 1, 500))
            .ToListAsync(ct);

    public async Task<bool> TryRevokeExpiredAsync(
        SessionId sessionId,
        UserId actorUserId,
        DateTimeOffset observedExpiry,
        DateTimeOffset revokedAt,
        CancellationToken ct = default)
    {
        var updated = await _context.SessionAccessOverrides
            .Where(accessOverride =>
                accessOverride.SessionId == sessionId
                && accessOverride.ActorUserId == actorUserId
                && accessOverride.RevokedAt == null
                && accessOverride.ExpiresAt == observedExpiry
                && accessOverride.ExpiresAt <= revokedAt)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    accessOverride => accessOverride.RevokedAt,
                    revokedAt),
                ct);
        return updated == 1;
    }
}
