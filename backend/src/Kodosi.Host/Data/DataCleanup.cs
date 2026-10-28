using Kodosi.Accounts;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Data;

internal sealed class DataCleanup(IServiceScopeFactory scopes, TimeProvider clock, ILogger<DataCleanup> logger)
    : PeriodicWork(TimeSpan.FromSeconds(30), clock, logger)
{
    internal static readonly TimeSpan LinkRequestMemory = TimeSpan.FromDays(1);

    protected internal override async Task RunAsync(CancellationToken ct)
    {
        await using var scope = scopes.CreateAsyncScope();
        var db = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
        var now = Clock.GetUtcNow();
        await db.Sessions.Where(x => x.Ended && x.ExpiresAt <= now).ExecuteDeleteAsync(ct);
        await db.DeviceLinks.Where(x => x.ExpiresAt < now - LinkRequestMemory).ExecuteDeleteAsync(ct);
        await db.DeletedAccounts.Where(x => x.DeletedAt < now - AccountService.DeletionMemory).ExecuteDeleteAsync(ct);
    }
}
