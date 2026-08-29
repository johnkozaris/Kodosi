using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence;

public sealed class AccessOverrideExpiryDurabilityCoordinator(
    IDbContextFactory<KodosiDbContext> contextFactory,
    TimeProvider timeProvider)
    : IAccessOverrideExpiryDurabilityCoordinator
{
    public async Task<IReadOnlyList<AccessOverrideExpiryEnforcementWork>> GetPendingEnforcementAsync(
        int limit,
        CancellationToken ct = default,
        AccessOverrideExpiryEnforcementCursor? before = null)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(limit, 1);
        await using var context = await contextFactory.CreateDbContextAsync(ct);
        var query = context.Set<AccessOverrideAuditEntry>()
            .AsNoTracking()
            .Where(entry =>
                entry.RealtimeEnforcedAt == null
                && entry.Action == AccessOverrideAuditAction.Revoked
                && entry.Reason == AccessOverrideAuditReason.Expired
                && entry.SessionIncarnationId != null
                && entry.SessionStartedAt != null
                && entry.ExpectedExpiresAt != null
                && entry.ExpectedRevokedAt != null);
        if (before is { } cursor)
        {
            query = query.Where(entry =>
                entry.OccurredAt < cursor.OccurredAt
                || entry.OccurredAt == cursor.OccurredAt
                    && entry.Id.CompareTo(cursor.AuditEntryId) < 0);
        }
        var entries = await query
            .OrderByDescending(entry => entry.OccurredAt)
            .ThenByDescending(entry => entry.Id)
            .Take(limit)
            .ToListAsync(ct);

        return entries
            .Select(entry => new AccessOverrideExpiryEnforcementWork(
                entry.Id,
                entry.OccurredAt,
                entry.SessionId,
                entry.GranteeUserId,
                entry.SessionIncarnationId!.Value,
                entry.SessionStartedAt!.Value,
                entry.ExpectedExpiresAt!.Value,
                entry.ExpectedRevokedAt!.Value))
            .ToList();
    }

    public async Task<bool> ExecuteIfCurrentAsync(
        AccessOverrideExpiryEnforcementWork work,
        Func<CancellationToken, Task> enforce,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(enforce);
        await using var context = await contextFactory.CreateDbContextAsync(ct);
        await using var transaction = await context.Database.BeginTransactionAsync(ct);

        var accessOverride = await context.SessionAccessOverrides
            .FromSqlInterpolated($"""
                SELECT *
                FROM session_access_overrides
                WHERE session_id = {work.SessionId.Value}
                  AND actor_user_id = {work.GranteeUserId.Value}
                  AND expires_at = {work.ExpectedExpiresAt}
                  AND revoked_at = {work.ExpectedRevokedAt}
                FOR UPDATE
                """)
            .SingleOrDefaultAsync(ct);
        var pending = await context.Set<AccessOverrideAuditEntry>()
            .AsNoTracking()
            .AnyAsync(entry =>
                entry.Id == work.AuditEntryId
                && entry.RealtimeEnforcedAt == null
                && entry.Action == AccessOverrideAuditAction.Revoked
                && entry.Reason == AccessOverrideAuditReason.Expired
                && entry.SessionId == work.SessionId
                && entry.GranteeUserId == work.GranteeUserId
                && entry.SessionIncarnationId == work.SessionIncarnationId
                && entry.SessionStartedAt == work.SessionStartedAt
                && entry.ExpectedExpiresAt == work.ExpectedExpiresAt
                && entry.ExpectedRevokedAt == work.ExpectedRevokedAt,
                ct);
        if (!pending || accessOverride is null)
        {
            await transaction.CommitAsync(ct);
            return false;
        }

        await enforce(ct);
        await transaction.CommitAsync(ct);
        return true;
    }

    public async Task CompleteEnforcementAsync(
        Guid auditEntryId,
        CancellationToken ct = default)
    {
        await using var context = await contextFactory.CreateDbContextAsync(ct);
        _ = await context.Set<AccessOverrideAuditEntry>()
            .Where(entry => entry.Id == auditEntryId
                && entry.RealtimeEnforcedAt == null)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    entry => entry.RealtimeEnforcedAt,
                    timeProvider.GetUtcNow()),
                ct);
    }
}
