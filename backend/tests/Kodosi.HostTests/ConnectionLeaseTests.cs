using Kodosi.Data;
using Kodosi.TerminalConnections;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging.Abstractions;
using Npgsql;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class ConnectionLeaseTests(PostgresFixture postgres)
{
    private static DatabaseConnection Database(string connection) => DatabaseConnection.From(new ConfigurationBuilder()
        .AddInMemoryCollection(new Dictionary<string, string?> { ["ConnectionStrings:Kodosi"] = connection }).Build());

    private static ConnectionLease Lease(string connection, TimeSpan wait) =>
        new(Database(connection), new ConnectionDirectory(), new Lifetime(), TimeProvider.System, NullLogger<ConnectionLease>.Instance) { AcquireWait = wait };

    [Fact]
    public async Task OneProcessHoldsTheLeaseAndTheNextStartGetsItWhenTheHolderEnds()
    {
        var ct = TestContext.Current.CancellationToken;
        var connection = await postgres.CreateDatabaseAsync(ct);
        await using var first = Lease(connection, TimeSpan.Zero);
        await first.AcquireAsync(ct);
        Assert.True(first.Held);
        await using var second = Lease(connection, TimeSpan.FromSeconds(1));
        var refused = await Assert.ThrowsAsync<InvalidOperationException>(() => second.AcquireAsync(ct));
        Assert.Equal("Another Kodosi backend owns the terminal connection lease.", refused.Message);
        Assert.False(second.Held);
        await first.DisposeAsync();
        await using var third = Lease(connection, TimeSpan.FromSeconds(5));
        await third.AcquireAsync(ct);
        Assert.True(third.Held);
    }

    [Fact]
    public async Task ALeaseStopsOnceWhenItStopsTwiceAtOnceOrAfterItsDisposal()
    {
        var ct = TestContext.Current.CancellationToken;
        var lease = Lease("Host=db.test;Database=kodosi", TimeSpan.Zero);
        await Task.WhenAll(lease.StopAsync(ct), lease.StopAsync(ct), lease.DisposeAsync().AsTask());
        await lease.StopAsync(ct);
        await lease.DisposeAsync();
    }

    [Fact]
    public void ThePoolHasALimitThatTheConnectionStringCanChangeAndTheLeaseUsesNoPool()
    {
        const string server = "Host=db.test;Database=kodosi;Username=kodosi";
        var plain = Database(server);
        Assert.Equal(DatabaseConnection.DefaultMaxPoolSize, new NpgsqlConnectionStringBuilder(plain.ConnectionString).MaxPoolSize);
        Assert.False(new NpgsqlConnectionStringBuilder(plain.LeaseConnectionString).Pooling);
        foreach (var setting in new[] { "Maximum Pool Size=80", "MaxPoolSize=80", "maximum pool size=80" })
            Assert.Equal(80, new NpgsqlConnectionStringBuilder(Database($"{server};{setting}").ConnectionString).MaxPoolSize);
    }

    private sealed class Lifetime : IHostApplicationLifetime
    {
        public CancellationToken ApplicationStarted => CancellationToken.None;
        public CancellationToken ApplicationStopping => CancellationToken.None;
        public CancellationToken ApplicationStopped => CancellationToken.None;
        public void StopApplication() { }
    }
}
