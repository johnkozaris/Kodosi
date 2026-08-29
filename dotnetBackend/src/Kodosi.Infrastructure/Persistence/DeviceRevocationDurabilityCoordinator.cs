using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence;

public sealed class DeviceRevocationDurabilityCoordinator(
    IDbContextFactory<KodosiDbContext> contextFactory)
    : IDeviceRevocationDurabilityCoordinator
{
    public async Task<IReadOnlyList<DeviceRevocationEnforcementWork>> GetPendingEnforcementAsync(
        int limit,
        CancellationToken ct = default)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(limit, 1);
        await using var context = await contextFactory.CreateDbContextAsync(ct);
        var operationIds = await context.Set<DeviceRevocationAuditEntry>()
            .AsNoTracking()
            .Where(entry => entry.RealtimeEnforcedAt == null)
            .OrderBy(entry => entry.OccurredAt)
            .ThenBy(entry => entry.RevocationId)
            .Select(entry => entry.RevocationId)
            .Distinct()
            .Take(limit)
            .ToListAsync(ct);
        if (operationIds.Count == 0)
        {
            return [];
        }
        var rows = await context.Set<DeviceRevocationAuditEntry>()
            .AsNoTracking()
            .Where(entry => operationIds.Contains(entry.RevocationId)
                && entry.RealtimeEnforcedAt == null)
            .OrderBy(entry => entry.OccurredAt)
            .ThenBy(entry => entry.Id)
            .ToListAsync(ct);
        return rows
            .GroupBy(entry => entry.RevocationId)
            .Select(group => new DeviceRevocationEnforcementWork(
                group.Key,
                group.First().ActorUserId,
                group.First().IdentityRevision,
                group.First().NewGeneration,
                group.Select(entry => entry.RevokedDeviceId)
                    .Distinct(StringComparer.Ordinal)
                    .Order(StringComparer.Ordinal)
                    .ToList(),
                group.SelectMany(entry => entry.GetAffectedSessionTargets())
                    .Distinct()
                    .OrderBy(target => target.SessionId.Value)
                    .ThenBy(target => target.IncarnationId)
                    .ToList()))
            .ToList();
    }

    public async Task<bool> ExecuteIfCurrentAsync(
        DeviceRevocationEnforcementWork work,
        Func<CancellationToken, Task> enforce,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(enforce);
        await using var context = await contextFactory.CreateDbContextAsync(ct);
        await using var transaction = await context.Database.BeginTransactionAsync(ct);
        await new PostgresUserLifecycleLock(context).AcquireAsync(work.UserId, ct);
        var current = await context.Users
            .AsNoTracking()
            .Where(user => user.Id == work.UserId)
            .Select(user => new { user.IdentityRevision, user.IdentityIncarnationId })
            .SingleOrDefaultAsync(ct);
        var currentGeneration = await context.UserDeviceLists
            .AsNoTracking()
            .Where(list => list.UserId == work.UserId)
            .MaxAsync(list => (long?)list.Generation, ct);
        if (current is null
            || current.IdentityIncarnationId is null
            || current.IdentityRevision != work.IdentityRevision
            || currentGeneration != work.DeviceListGeneration)
        {
            await transaction.CommitAsync(ct);
            return false;
        }

        await enforce(ct);
        await transaction.CommitAsync(ct);
        return true;
    }

    public async Task CompleteEnforcementAsync(
        Guid revocationId,
        CancellationToken ct = default)
    {
        await using var context = await contextFactory.CreateDbContextAsync(ct);
        _ = await context.Set<DeviceRevocationAuditEntry>()
            .Where(entry => entry.RevocationId == revocationId
                && entry.RealtimeEnforcedAt == null)
            .ExecuteUpdateAsync(
                setters => setters.SetProperty(
                    entry => entry.RealtimeEnforcedAt,
                    DateTimeOffset.UtcNow),
                ct);
    }
}
