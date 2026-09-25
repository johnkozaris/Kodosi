using Kodosi.Admission;
using Kodosi.Data;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Sessions;

public sealed class PublicationCleanup(IServiceScopeFactory scopes, AdmissionGate gate, ConnectionDirectory connections,
    TimeProvider clock, ILogger<PublicationCleanup> logger) : BackgroundService
{
    internal static readonly TimeSpan GracePeriod = TimeSpan.FromMinutes(2);
    private readonly DateTimeOffset restartGraceUntil = clock.GetUtcNow() + GracePeriod;

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        using var timer = new PeriodicTimer(TimeSpan.FromSeconds(30), clock);
        try
        {
            while (await timer.WaitForNextTickAsync(stoppingToken))
            {
                try { await SweepAsync(stoppingToken); }
                catch (Exception error) when (error is DbUpdateException or Npgsql.NpgsqlException)
                { logger.LogWarning(error, "Publication expiry will retry on the next sweep."); }
            }
        }
        catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested) { }
    }

    internal async Task SweepAsync(CancellationToken ct)
    {
        foreach (var batch in connections.OnlineSessionIds().Chunk(128))
        {
            using var held = await gate.EnterAsync(ct);
            await using var scope = scopes.CreateAsyncScope();
            var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
            var active = batch.Where(connections.HostOnline).ToArray();
            var expires = clock.GetUtcNow() + GracePeriod;
            await db.Sessions.Where(s => !s.Ended && active.Contains(s.Id))
                .ExecuteUpdateAsync(set => set.SetProperty(s => s.ExpiresAt, expires), ct);
        }
        if (clock.GetUtcNow() < restartGraceUntil) return;
        using var admission = await gate.EnterAsync(ct);
        await using var cleanupScope = scopes.CreateAsyncScope();
        var store = cleanupScope.ServiceProvider.GetRequiredService<KodosiDbContext>();
        var now = clock.GetUtcNow();
        var online = connections.OnlineSessionIds();
        var expired = await store.Sessions.AsNoTracking().Where(s => s.ExpiresAt <= now && !online.Contains(s.Id))
            .OrderBy(s => s.ExpiresAt).ThenBy(s => s.Id).Take(128).ToListAsync(ct);
        if (expired.Count == 0) return;
        var ids = expired.Select(s => s.Id).ToArray();
        var users = await store.SessionMembers.Where(m => ids.Contains(m.SessionId)).Select(m => m.UserId).Distinct().ToListAsync(ct);
        foreach (var session in expired) connections.RemoveSession(session.Id);
        await store.Sessions.Where(s => ids.Contains(s.Id)).ExecuteDeleteAsync(ct);
        foreach (var user in users.Union(expired.Select(s => s.OwnerUserId))) connections.Notify(user, "sessions");
    }
}
