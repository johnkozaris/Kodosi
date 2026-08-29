using Kodosi.Domain;
using Kodosi.Infrastructure.RateLimiting;

namespace Kodosi.HostTests;

public sealed class InMemoryFriendRequestThrottleTests
{
    private static UserId U(string hex) => UserId.From(Guid.Parse(hex));

    private static readonly UserId Alice = U("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
    private static readonly UserId Bob = U("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
    private static readonly UserId Carol = U("cccccccc-cccc-cccc-cccc-cccccccccccc");

    [Fact]
    public void FirstClaim_Succeeds()
    {
        var throttle = new InMemoryFriendRequestThrottle(new ManualClock());

        Assert.Null(throttle.TryClaim(Alice, Bob));
    }

    [Fact]
    public void ImmediateRepeatClaim_SameTarget_Denies_WithRetryAfter()
    {
        var clock = new ManualClock();
        var throttle = new InMemoryFriendRequestThrottle(clock);

        Assert.Null(throttle.TryClaim(Alice, Bob));
        clock.Advance(TimeSpan.FromMinutes(5));

        var retryAfter = throttle.TryClaim(Alice, Bob);

        Assert.NotNull(retryAfter);
        Assert.True(retryAfter.Value > TimeSpan.FromHours(23));
        Assert.True(retryAfter.Value < TimeSpan.FromHours(24));
    }

    [Fact]
    public void DifferentTarget_SameSender_Allowed()
    {
        var throttle = new InMemoryFriendRequestThrottle(new ManualClock());

        Assert.Null(throttle.TryClaim(Alice, Bob));
        Assert.Null(throttle.TryClaim(Alice, Carol));
    }

    [Fact]
    public void ClaimRefreshes_AfterWindow_Expires()
    {
        var clock = new ManualClock();
        var throttle = new InMemoryFriendRequestThrottle(clock);

        Assert.Null(throttle.TryClaim(Alice, Bob));
        clock.Advance(TimeSpan.FromHours(25));

        Assert.Null(throttle.TryClaim(Alice, Bob));
    }

    [Fact]
    public void Sweep_Removes_Expired_Entries_Only()
    {
        var clock = new ManualClock();
        var throttle = new InMemoryFriendRequestThrottle(clock);

        Assert.Null(throttle.TryClaim(Alice, Bob));
        clock.Advance(TimeSpan.FromHours(25));
        Assert.Null(throttle.TryClaim(Alice, Carol));

        throttle.Sweep();

        Assert.Null(throttle.TryClaim(Alice, Bob));
        Assert.NotNull(throttle.TryClaim(Alice, Carol));
    }

    [Fact]
    public void CapacityRejectsNewPairUntilAnEntryExpires()
    {
        var clock = new ManualClock();
        var throttle = new InMemoryFriendRequestThrottle(clock, maxEntries: 2);

        Assert.Null(throttle.TryClaim(Alice, Bob));
        Assert.Null(throttle.TryClaim(Alice, Carol));
        Assert.Equal(TimeSpan.FromSeconds(1), throttle.TryClaim(Bob, Carol));

        clock.Advance(TimeSpan.FromHours(25));

        Assert.Null(throttle.TryClaim(Bob, Carol));
    }

    [Fact]
    public void AsymmetricPair_Treated_Independently()
    {
        var throttle = new InMemoryFriendRequestThrottle(new ManualClock());

        Assert.Null(throttle.TryClaim(Alice, Bob));
        Assert.Null(throttle.TryClaim(Bob, Alice));
    }

    private sealed class ManualClock : TimeProvider
    {
        private DateTimeOffset _now = new(2026, 4, 18, 12, 0, 0, TimeSpan.Zero);

        public override DateTimeOffset GetUtcNow() => _now;

        public void Advance(TimeSpan delta) => _now = _now.Add(delta);
    }
}
