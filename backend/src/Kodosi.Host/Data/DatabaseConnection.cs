using System.Data.Common;
using Npgsql;

namespace Kodosi.Data;

public sealed class DatabaseConnection
{
    public const int DefaultMaxPoolSize = 32;

    private DatabaseConnection(string pooled, string lease)
    {
        ConnectionString = pooled;
        LeaseConnectionString = lease;
    }

    public string ConnectionString { get; }

    public string LeaseConnectionString { get; }

    public static DatabaseConnection From(IConfiguration config)
    {
        var text = config.GetConnectionString("Kodosi") ?? throw new InvalidOperationException("ConnectionStrings:Kodosi is required.");
        var pooled = new NpgsqlConnectionStringBuilder(text);
        if (!SetsPoolSize(text)) pooled.MaxPoolSize = DefaultMaxPoolSize;
        var lease = new NpgsqlConnectionStringBuilder(text) { Pooling = false };
        return new DatabaseConnection(pooled.ConnectionString, lease.ConnectionString);
    }

    private static bool SetsPoolSize(string text) =>
        new DbConnectionStringBuilder { ConnectionString = text }.Keys.Cast<string>()
            .Any(key => key.Replace(" ", "", StringComparison.Ordinal).ToLowerInvariant() is "maximumpoolsize" or "maxpoolsize");
}
