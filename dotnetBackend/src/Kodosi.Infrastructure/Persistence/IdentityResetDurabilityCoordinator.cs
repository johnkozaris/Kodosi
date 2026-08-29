using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence;

public sealed class IdentityResetDurabilityCoordinator(
    IDbContextFactory<KodosiDbContext> contextFactory)
    : IIdentityResetDurabilityCoordinator
{
    private readonly IDbContextFactory<KodosiDbContext> _contextFactory = contextFactory;

    public async Task<IdentityResetCommitReconciliation> ReconcileCommitAsync(
        Guid resetId,
        UserId userId,
        IReadOnlyCollection<string> expectedRemovedDeviceIds,
        CancellationToken ct = default)
    {
        await using var context = await _contextFactory.CreateDbContextAsync(ct);
        var audit = await context.IdentityResetAuditEntries
            .AsNoTracking()
            .SingleOrDefaultAsync(entry => entry.Id == resetId, ct);
        if (audit is null)
        {
            return new IdentityResetCommitReconciliation(
                IdentityResetCommitState.NotCommitted);
        }

        var expectedDevices = expectedRemovedDeviceIds
            .Distinct(StringComparer.Ordinal)
            .Order(StringComparer.Ordinal)
            .ToArray();
        if (audit.UserId != userId
            || !audit.RemovedDeviceIds.SequenceEqual(
                expectedDevices,
                StringComparer.Ordinal))
        {
            return Inconsistent("The durable audit identity or removed-device set did not match.");
        }

        if (await context.UserDevices
                .AsNoTracking()
                .AnyAsync(
                    device => device.UserId == userId
                        && audit.RemovedDeviceIds.Contains(device.DeviceId)
                        && device.CreatedAt <= audit.ResetAt,
                    ct)
            || await context.UserDeviceLists
                .AsNoTracking()
                .AnyAsync(
                    list => list.UserId == userId
                        && list.CreatedAt <= audit.ResetAt,
                    ct))
        {
            return Inconsistent("Durable device state was not fully reset.");
        }

        var endedTargets = audit.GetEndedSessionTargets();
        if (audit.EndedSessionIds.Length > 0)
        {
            if (endedTargets.Count != audit.EndedSessionIds.Length)
            {
                return Inconsistent("Audited session ends lacked exact incarnation targets.");
            }
            var endedIds = endedTargets
                .Select(target => target.SessionId.Value)
                .ToArray();
            var endedSessions = await context.Sessions
                .AsNoTracking()
                .Where(session => endedIds.Contains(session.Id.Value))
                .Select(session => new
                {
                    session.Id,
                    session.IncarnationId,
                    session.Status,
                })
                .ToListAsync(ct);
            var exactEndedTargets = endedTargets
                .Select(target => (target.SessionId, target.IncarnationId))
                .ToHashSet();
            var endedCount = endedSessions.Count(session =>
                session.Status == SessionStatus.Ended
                && exactEndedTargets.Contains((session.Id, session.IncarnationId)));
            if (endedCount != endedTargets.Count)
            {
                return Inconsistent("One or more audited session ends were not durable.");
            }
        }

        return new IdentityResetCommitReconciliation(
            IdentityResetCommitState.Committed,
            ToWork(audit));
    }

    public async Task<IReadOnlyList<IdentityResetEnforcementWork>>
        GetPendingEnforcementAsync(
            int limit,
            CancellationToken ct = default)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(limit, 1);

        await using var context = await _contextFactory.CreateDbContextAsync(ct);
        var pending = await context.IdentityResetAuditEntries
            .AsNoTracking()
            .Where(entry => entry.RealtimeEnforcedAt == null)
            .OrderBy(entry => entry.ResetAt)
            .ThenBy(entry => entry.Id)
            .Take(limit)
            .ToListAsync(ct);
        return pending.Select(ToWork).ToList();
    }

    public async Task<bool> ExecuteIfCurrentAsync(
        IdentityResetEnforcementWork work,
        Func<CancellationToken, Task> enforce,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(enforce);
        await using var context = await _contextFactory.CreateDbContextAsync(ct);
        await using var transaction = await context.Database.BeginTransactionAsync(ct);
        await new PostgresUserLifecycleLock(context).AcquireAsync(work.UserId, ct);
        var currentRevision = await context.Users
            .AsNoTracking()
            .Where(user => user.Id == work.UserId)
            .Select(user => user.IdentityRevision)
            .SingleOrDefaultAsync(ct);
        if (currentRevision != work.IdentityRevision)
        {
            await transaction.CommitAsync(ct);
            return false;
        }

        await enforce(ct);
        await transaction.CommitAsync(ct);
        return true;
    }

    public async Task CompleteEnforcementAsync(
        Guid resetId,
        CancellationToken ct = default)
    {
        await using var context = await _contextFactory.CreateDbContextAsync(ct);
        _ = await context.IdentityResetAuditEntries
            .Where(entry => entry.Id == resetId
                && entry.RealtimeEnforcedAt == null)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    entry => entry.RealtimeEnforcedAt,
                    DateTimeOffset.UtcNow),
                ct);
    }

    private static IdentityResetCommitReconciliation Inconsistent(string detail) =>
        new(IdentityResetCommitState.Inconsistent, Detail: detail);

    private static IdentityResetEnforcementWork ToWork(
        IdentityResetAuditEntry audit)
    {
        var sessionTargets = audit.GetSessionTargets();
        var endedTargets = audit.GetEndedSessionTargets();
        return new IdentityResetEnforcementWork(
            audit.Id,
            audit.UserId,
            audit.IdentityRevision,
            [.. audit.RemovedDeviceIds],
            audit.EndedSessionIds.Select(SessionId.From).ToList(),
            audit.SessionsWithRevokedKeys.Select(SessionId.From).ToList())
        {
            SessionTargets = sessionTargets,
            EndedSessions = endedTargets
                .Select(target => new IdentityResetEndedSession(
                    new SessionDiscoveryTarget(
                        target.SessionId,
                        target.OwnerUserId,
                        target.Scope,
                        target.RoomId,
                        target.IncarnationId),
                    target.StartedAt))
                .ToList(),
        };
    }
}
