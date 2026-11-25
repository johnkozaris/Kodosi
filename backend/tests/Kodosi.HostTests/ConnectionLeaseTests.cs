using Kodosi.Data;
using Kodosi.TerminalConnections;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging.Abstractions;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class ConnectionLeaseTests(PostgresFixture postgres)
{
    private static DatabaseConnection Database(string connection) => DatabaseConnection.From(new ConfigurationBuilder()
        .AddInMemoryCollection(new Dictionary<string, string?> { ["ConnectionStrings:Kodosi"] = connection }).Build());

    private static ConnectionLease Lease(string connection, TimeSpan wait) =>
        new(Database(connection), new ConnectionDirectory(TimeProvider.System), new Lifetime(), TimeProvider.System, NullLogger<ConnectionLease>.Instance) { AcquireWait = wait };

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

    private sealed class Lifetime : IHostApplicationLifetime
    {
        public CancellationToken ApplicationStarted => CancellationToken.None;
        public CancellationToken ApplicationStopping => CancellationToken.None;
        public CancellationToken ApplicationStopped => CancellationToken.None;
        public void StopApplication() { }
    }
}
