using Kodosi.Infrastructure.RateLimiting;

namespace Kodosi.HostTests;

public sealed class InMemoryDeviceLinkPollThrottleTests
{
    [Fact]
    public void Limits_Each_DeviceCode_Independently()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var throttle = new InMemoryDeviceLinkPollThrottle(
            permitLimit: 2,
            window: TimeSpan.FromMinutes(1),
            clock);

        Assert.Null(throttle.TryClaim("device-a"));
        Assert.Null(throttle.TryClaim("device-a"));
        Assert.NotNull(throttle.TryClaim("device-a"));
        Assert.Null(throttle.TryClaim("device-b"));
    }

    [Fact]
    public void Window_Expires_And_Sweep_Releases_State()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var throttle = new InMemoryDeviceLinkPollThrottle(
            permitLimit: 1,
            window: TimeSpan.FromMinutes(1),
            clock);
        Assert.Null(throttle.TryClaim("device-a"));
        Assert.NotNull(throttle.TryClaim("device-a"));

        clock.Advance(TimeSpan.FromMinutes(2));
        throttle.Sweep();

        Assert.Null(throttle.TryClaim("device-a"));
    }
}
