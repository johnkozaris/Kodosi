using Kodosi.Data;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Sessions;

public sealed class PublicationCleanup(IServiceScopeFactory scopes, TimeProvider clock, ILogger<PublicationCleanup> logger) : BackgroundService
{
    internal static readonly TimeSpan GracePeriod = TimeSpan.FromMinutes(2);

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        using var timer = new PeriodicTimer(TimeSpan.FromSeconds(30), clock);
        try
        {
            while (await timer.WaitForNextTickAsync(stoppingToken))
            {
                try { await SweepAsync(stoppingToken); }
                catch (Exception error) when (error is DbUpdateException or Npgsql.NpgsqlException
                                              or InvalidOperationException { InnerException: Npgsql.NpgsqlException or TimeoutException })
                { logger.LogWarning(error, "Ended publication cleanup will retry on the next sweep."); }
            }
        }
        catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested) { }
    }

    internal async Task SweepAsync(CancellationToken ct)
    {
        await using var scope = scopes.CreateAsyncScope();
        var now = clock.GetUtcNow();
        var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
        await db.Sessions.Where(s => s.Ended && s.ExpiresAt <= now).ExecuteDeleteAsync(ct);
        var forgotten = now - Accounts.AccountService.DeletionMemory;
        await db.DeletedAccounts.Where(x => x.DeletedAt < forgotten).ExecuteDeleteAsync(ct);
    }
}
