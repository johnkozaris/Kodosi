using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Infrastructure;
using Microsoft.EntityFrameworkCore.Migrations;
using Kodosi.Application;
using Kodosi.Infrastructure.Persistence;

namespace Kodosi.Host;

public static class DatabaseInitializerHostedService
{
    public static async Task InitializeAsync(IServiceProvider services)
    {
        using var scope = services.CreateScope();
        var dbContext = scope.ServiceProvider.GetRequiredService<KodosiDbContext>();
        var logger = scope.ServiceProvider.GetRequiredService<ILoggerFactory>()
            .CreateLogger("DatabaseInitializerHostedService");

        var applied = (await dbContext.Database.GetAppliedMigrationsAsync()).ToHashSet(StringComparer.Ordinal);
        var expected = dbContext.Database.GetMigrations().ToHashSet(StringComparer.Ordinal);
        var unknownApplied = applied.Except(expected, StringComparer.Ordinal).ToArray();
        if (unknownApplied.Length > 0)
        {


            var names = string.Join(", ", unknownApplied);
            throw new InvalidOperationException(
                $"Database schema is ahead of this build. Unknown applied migrations: {names}. "
                + "This process will not start against an unfamiliar schema; deploy a build that includes these migrations.");
        }

        var pending = (await dbContext.Database.GetPendingMigrationsAsync()).ToArray();
        if (pending.Length == 0)
        {
            logger.LogInformation("Database schema is up-to-date; no migrations to apply");
            return;
        }

        logger.LogInformation(
            "Applying {Count} pending migrations: {Names}",
            pending.Length,
            string.Join(", ", pending));

        if (pending.Contains(
                DeviceCertificateAuthorityMigrationPreflight.Migration,
                StringComparer.Ordinal))
        {
            if (pending.Contains(
                    DeviceCertificateAuthorityMigrationPreflight.DeviceListCollapseMigration,
                    StringComparer.Ordinal))
            {
                await dbContext.GetService<IMigrator>().MigrateAsync(
                    DeviceCertificateAuthorityMigrationPreflight.DeviceListCollapsePredecessor);
                await DeviceCertificateAuthorityMigrationPreflight
                    .RequireLegacyListProjectionParityAsync(dbContext);
            }
            await DeviceCertificateAuthorityMigrationPreflight.RunAsync(
                dbContext,
                scope.ServiceProvider.GetRequiredService<IPopSignatureVerifier>());
        }

        await dbContext.Database.MigrateAsync();
        logger.LogInformation("Applied database migrations");
    }
}
