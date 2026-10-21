using Kodosi.Data;
using Microsoft.EntityFrameworkCore;
using Npgsql;
using Testcontainers.PostgreSql;
using Xunit;

namespace Kodosi.HostTests;

[CollectionDefinition("PostgreSQL")]
public sealed class PostgresCollection : ICollectionFixture<PostgresFixture>;

public sealed class PostgresFixture : IAsyncLifetime
{
    private readonly PostgreSqlContainer container = new PostgreSqlBuilder("postgres:18.6-alpine@sha256:77f585114c32fbca283dc835b0596f4e52b51b4c6662d7810b2f4084f60a1873").Build();
    public ValueTask InitializeAsync() => new(container.StartAsync());
    public ValueTask DisposeAsync() => container.DisposeAsync();

    public async Task<string> CreateDatabaseAsync(CancellationToken ct)
    {
        var name = "test_" + Guid.NewGuid().ToString("N");
        await using var connection = new NpgsqlConnection(container.GetConnectionString());
        await connection.OpenAsync(ct);
        await using var command = new NpgsqlCommand($"CREATE DATABASE {name}", connection);
        await command.ExecuteNonQueryAsync(ct);
        return new NpgsqlConnectionStringBuilder(container.GetConnectionString()) { Database = name }.ConnectionString;
    }

    public static KodosiDbContext Context(string connection) => new(new DbContextOptionsBuilder<KodosiDbContext>().UseNpgsql(connection).Options);
}
