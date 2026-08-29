using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.Diagnostics.HealthChecks;
using Kodosi.Infrastructure.Persistence;

namespace Kodosi.Host.Health;

public sealed class DatabaseHealthCheck(IDbContextFactory<KodosiDbContext> dbContextFactory) : IHealthCheck
{
    private readonly IDbContextFactory<KodosiDbContext> _dbContextFactory = dbContextFactory;

    public async Task<HealthCheckResult> CheckHealthAsync(
        HealthCheckContext context,
        CancellationToken cancellationToken = default)
    {
        await using var dbContext = await _dbContextFactory.CreateDbContextAsync(cancellationToken);
        var canConnect = await dbContext.Database.CanConnectAsync(cancellationToken);

        return canConnect
            ? HealthCheckResult.Healthy("PostgreSQL is reachable.")
            : HealthCheckResult.Unhealthy("PostgreSQL is unreachable.");
    }
}
