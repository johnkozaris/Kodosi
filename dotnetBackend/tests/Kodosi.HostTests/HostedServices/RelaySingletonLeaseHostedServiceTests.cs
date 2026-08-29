using Kodosi.Host;
using Microsoft.Extensions.Logging.Abstractions;
using Npgsql;
using Testcontainers.PostgreSql;

namespace Kodosi.HostTests;

public sealed class RelaySingletonLeaseHostedServiceTests
{
    [Fact]
    public async Task Starting_The_Same_Service_Twice_Does_Not_Leak_A_Lock()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_relay_lease_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        await using var dataSource = NpgsqlDataSource.Create(postgres.GetConnectionString());
        var lifetime = new TestApplicationLifetime();
        var first = new RelaySingletonLeaseHostedService(
            dataSource,
            lifetime,
            NullLogger<RelaySingletonLeaseHostedService>.Instance);

        await first.StartAsync(TestContext.Current.CancellationToken);
        await first.StartAsync(TestContext.Current.CancellationToken);
        await first.StopAsync(TestContext.Current.CancellationToken);

        var replacement = new RelaySingletonLeaseHostedService(
            dataSource,
            lifetime,
            NullLogger<RelaySingletonLeaseHostedService>.Instance);
        await replacement.StartAsync(TestContext.Current.CancellationToken);
        await replacement.StopAsync(TestContext.Current.CancellationToken);
    }

    [Fact]
    public async Task StopAsync_Joins_The_Monitor_And_Releases_The_Lease_When_The_Stop_Token_Is_Canceled()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_relay_lease_cancel_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        await using var dataSource = NpgsqlDataSource.Create(postgres.GetConnectionString());
        var lifetime = new TestApplicationLifetime();
        using var first = new RelaySingletonLeaseHostedService(
            dataSource,
            lifetime,
            NullLogger<RelaySingletonLeaseHostedService>.Instance);
        await first.StartAsync(TestContext.Current.CancellationToken);

        using var canceledStop = new CancellationTokenSource();
        canceledStop.Cancel();
        await first.StopAsync(canceledStop.Token);

        using var replacement = new RelaySingletonLeaseHostedService(
            dataSource,
            lifetime,
            NullLogger<RelaySingletonLeaseHostedService>.Instance);
        await replacement.StartAsync(TestContext.Current.CancellationToken);
        await replacement.StopAsync(TestContext.Current.CancellationToken);
    }

    [Fact]
    public async Task Concurrent_Stops_Are_Idempotent()
    {
        await using var postgres = new PostgreSqlBuilder("postgres:16-alpine")
            .WithDatabase("kodosi_relay_lease_concurrent_stop_test")
            .WithUsername("kodosi")
            .WithPassword("kodosi-test-password")
            .Build();
        await postgres.StartAsync(TestContext.Current.CancellationToken);
        await using var dataSource = NpgsqlDataSource.Create(postgres.GetConnectionString());
        using var service = new RelaySingletonLeaseHostedService(
            dataSource,
            new TestApplicationLifetime(),
            NullLogger<RelaySingletonLeaseHostedService>.Instance);
        await service.StartAsync(TestContext.Current.CancellationToken);

        await Task.WhenAll(
            service.StopAsync(CancellationToken.None),
            service.StopAsync(CancellationToken.None));

        using var replacement = new RelaySingletonLeaseHostedService(
            dataSource,
            new TestApplicationLifetime(),
            NullLogger<RelaySingletonLeaseHostedService>.Instance);
        await replacement.StartAsync(TestContext.Current.CancellationToken);
        await replacement.StopAsync(TestContext.Current.CancellationToken);
    }
}
