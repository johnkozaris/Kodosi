using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence;

public sealed class SessionIncarnationResolver(
    IDbContextFactory<KodosiDbContext> contextFactory)
    : IDeviceRevocationSessionResolver,
      IIdentityResetSessionResolver
{
    async Task<IReadOnlyList<DeviceRevocationSessionTarget>>
        IDeviceRevocationSessionResolver.GetCurrentTargetsAsync(
            IReadOnlyCollection<DeviceRevocationSessionTarget> targets,
            CancellationToken ct)
    {
        var current = await ResolveCurrentAsync(
            targets.Select(target => new SessionIncarnationTarget(
                target.SessionId,
                target.IncarnationId)),
            ct);
        return current
            .Select(target => new DeviceRevocationSessionTarget(
                target.SessionId,
                target.IncarnationId))
            .ToList();
    }

    async Task<IReadOnlyList<IdentityResetSessionTarget>>
        IIdentityResetSessionResolver.GetCurrentTargetsAsync(
            IReadOnlyCollection<IdentityResetSessionTarget> targets,
            CancellationToken ct)
    {
        var current = await ResolveCurrentAsync(
            targets.Select(target => new SessionIncarnationTarget(
                target.SessionId,
                target.IncarnationId)),
            ct);
        return current
            .Select(target => new IdentityResetSessionTarget(
                target.SessionId,
                target.IncarnationId))
            .ToList();
    }

    private async Task<IReadOnlyList<SessionIncarnationTarget>> ResolveCurrentAsync(
        IEnumerable<SessionIncarnationTarget> targets,
        CancellationToken ct)
    {
        var expected = targets.ToHashSet();
        if (expected.Count == 0)
        {
            return [];
        }

        var sessionIds = expected
            .Select(target => target.SessionId)
            .Distinct()
            .ToArray();
        await using var context = await contextFactory.CreateDbContextAsync(ct);
        var current = await context.Sessions
            .AsNoTracking()
            .Where(session => sessionIds.Contains(session.Id))
            .Select(session => new SessionIncarnationTarget(
                session.Id,
                session.IncarnationId))
            .ToListAsync(ct);
        return current
            .Where(expected.Contains)
            .OrderBy(target => target.SessionId.Value)
            .ThenBy(target => target.IncarnationId)
            .ToList();
    }

    private readonly record struct SessionIncarnationTarget(
        SessionId SessionId,
        Guid IncarnationId);
}
