using Microsoft.EntityFrameworkCore;

namespace Kodosi.Data;

internal static class DatabaseSetup
{
    public static async Task MigrateAsync(KodosiDbContext db, CancellationToken ct)
    {
        await db.Database.OpenConnectionAsync(ct);
        try
        {
            await using var inspect = db.Database.GetDbConnection().CreateCommand();
            inspect.CommandText = "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE'";
            var tables = Convert.ToInt64(await inspect.ExecuteScalarAsync(ct));
            if (tables > 0)
            {
                var applied = (await db.Database.GetAppliedMigrationsAsync(ct)).ToArray();
                var known = db.Database.GetMigrations().ToArray();
                if (applied.Length == 0 || applied.Except(known).Any())
                    throw new InvalidOperationException("This database uses an unsupported Kodosi schema. Select a fresh database; no data was changed.");
            }
            await db.Database.MigrateAsync(ct);
        }
        finally { await db.Database.CloseConnectionAsync(); }
    }

    public static async Task RequireCurrentAsync(KodosiDbContext db, CancellationToken ct)
    {
        if ((await db.Database.GetPendingMigrationsAsync(ct)).Any())
            throw new InvalidOperationException("The database schema is not current. Run the server with the argument 'migrate' first; no data was changed.");
    }
}
