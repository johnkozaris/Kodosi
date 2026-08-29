using System.Diagnostics;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging.Abstractions;
using Microsoft.Extensions.Options;
using Kodosi.Domain;
using Kodosi.Host;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class GracefulShutdownServiceTests
{
    [Fact]
    public async Task StopAsync_Returns_Quickly_When_No_Downstream_Connections()
    {
        var service = CreateService(out _, out _);
        var stopwatch = Stopwatch.StartNew();

        await service.StopAsync(CancellationToken.None);

        stopwatch.Stop();

        Assert.True(service.IsDraining);
        Assert.True(
            stopwatch.Elapsed < TimeSpan.FromSeconds(1),
            $"Expected shutdown without downstream clients to skip the drain wait, but it took {stopwatch.Elapsed}.");
    }

    [Fact]
    public async Task StopAsync_Completes_Downstream_And_Host_Queues_With_Server_Restarting_Reason()
    {
        var service = CreateService(out var broadcaster, out _);
        var sessionQueues = broadcaster.GetOrCreateSession(SessionId.New());
        var viewerQueue = sessionQueues.AddParticipantQueue("viewer-1");
        var hostQueue = sessionQueues.SetHostQueue("host-1");
        using var cts = new CancellationTokenSource(TimeSpan.FromMilliseconds(50));

        await service.StopAsync(cts.Token);

        Assert.True(service.IsDraining);
        Assert.Equal(QueueCompletionCause.Drain, viewerQueue.CompletionCause);
        Assert.Equal(CloseReason.ServerRestarting, hostQueue.CompletionReason);
    }

    [Fact]
    public async Task StopAsync_Waits_For_HostOnly_Connection_To_Unregister()
    {
        var service = CreateService(out var broadcaster, out var connections);
        var sessionId = SessionId.New();
        _ = broadcaster.GetOrCreateSession(sessionId).SetHostQueue("host-1");
        connections.RegisterHost(
            "host-1",
            UserId.New(),
            "device-1",
            sessionId);

        var stopping = service.StopAsync(CancellationToken.None);
        await Task.Delay(50, TestContext.Current.CancellationToken);
        Assert.False(stopping.IsCompleted);

        connections.Remove("host-1");
        await stopping.WaitAsync(
            TimeSpan.FromSeconds(1),
            TestContext.Current.CancellationToken);
    }

    [Fact]
    public async Task Drain_Begins_On_ApplicationStopping_Without_Waiting_For_StopAsync()
    {
        var lifetime = new TestApplicationLifetime();
        var service = CreateService(out var broadcaster, out _, lifetime);
        var hostQueue = broadcaster.GetOrCreateSession(SessionId.New()).SetHostQueue("host-1");

        Assert.False(service.IsDraining);
        lifetime.StopApplication();




        await WaitForAsync(() => service.IsDraining);
        await WaitForAsync(() => hostQueue.CompletionReason == CloseReason.ServerRestarting);
    }

    [Fact]
    public async Task StopAsync_Joins_Running_Drain_Instead_Of_Draining_Twice()
    {
        var lifetime = new TestApplicationLifetime();
        var service = CreateService(
            out var broadcaster,
            out var connections,
            out var metrics,
            lifetime);
        var sessionId = SessionId.New();
        _ = broadcaster.GetOrCreateSession(sessionId).SetHostQueue("host-1");
        connections.RegisterHost("host-1", UserId.New(), "device-1", sessionId);

        lifetime.StopApplication();
        await WaitForAsync(() => service.IsDraining);

        var stopping = service.StopAsync(CancellationToken.None);
        await Task.Delay(50, TestContext.Current.CancellationToken);


        Assert.False(stopping.IsCompleted);

        connections.Remove("host-1");
        await stopping.WaitAsync(
            TimeSpan.FromSeconds(5),
            TestContext.Current.CancellationToken);



        Assert.Equal(1, metrics.Snapshot().DrainStartedCount);
    }

    [Fact]
    public async Task StopAsync_Drains_When_ApplicationStopping_Never_Fires()
    {
        var service = CreateService(out var broadcaster, out _, new TestApplicationLifetime());
        var hostQueue = broadcaster.GetOrCreateSession(SessionId.New()).SetHostQueue("host-1");

        await service.StopAsync(CancellationToken.None);

        Assert.True(service.IsDraining);
        Assert.Equal(CloseReason.ServerRestarting, hostQueue.CompletionReason);
    }

    private static async Task WaitForAsync(Func<bool> condition)
    {
        var deadline = DateTimeOffset.UtcNow + TimeSpan.FromSeconds(5);
        while (!condition())
        {
            Assert.True(DateTimeOffset.UtcNow < deadline, "Timed out waiting for condition.");
            await Task.Delay(10, TestContext.Current.CancellationToken);
        }
    }

    private static GracefulShutdownHostedService CreateService(
        out SessionBroadcaster broadcaster,
        out ConnectionRegistry connections)
        => CreateService(
            out broadcaster,
            out connections,
            out _,
            new TestApplicationLifetime());

    private static GracefulShutdownHostedService CreateService(
        out SessionBroadcaster broadcaster,
        out ConnectionRegistry connections,
        TestApplicationLifetime lifetime)
        => CreateService(out broadcaster, out connections, out _, lifetime);

    private static GracefulShutdownHostedService CreateService(
        out SessionBroadcaster broadcaster,
        out ConnectionRegistry connections,
        out OperationalMetrics metrics,
        TestApplicationLifetime lifetime)
    {
        var runtimes = new LiveSessionStateDirectory();
        metrics = new OperationalMetrics(runtimes);
        connections = new ConnectionRegistry();
        var userEvents = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);

        var service = new GracefulShutdownHostedService(
            broadcaster,
            connections,
            userEvents,
            Options.Create(new HostOptions { ShutdownTimeout = TimeSpan.FromSeconds(30) }),
            metrics,
            lifetime,
            NullLogger<GracefulShutdownHostedService>.Instance);
        service.StartAsync(CancellationToken.None).GetAwaiter().GetResult();
        return service;
    }
}
