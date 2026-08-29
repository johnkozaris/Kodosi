using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class HostSessionStateTests
{
    [Fact]
    public void PendingHostFenceQueue_Rejects_NonCoalescing_Overflow()
    {
        var host = new HostSessionState();

        for (var index = 0; index < 1_024; index++)
        {
            Assert.True(host.TryQueuePendingHostFence(
                new PendingHostFence.AccessRevoked(UserId.New())));
        }

        Assert.False(host.TryQueuePendingHostFence(
            new PendingHostFence.AccessRevoked(UserId.New())));
    }

    [Fact]
    public void TryTimeoutHost_Cancels_Current_Host_Lifetime()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var host = new HostSessionState(clock);
        using var hostLifetime = new CancellationTokenSource();

        Assert.True(host.TryClaimHost("host-1", hostLifetime));
        clock.Advance(TimeSpan.FromMinutes(1));

        var timedOut = host.TryTimeoutHost(
            "host-1",
            TimeSpan.FromSeconds(15));

        Assert.True(timedOut);
        Assert.True(hostLifetime.IsCancellationRequested);
        Assert.False(host.HostConnected);
    }
}
