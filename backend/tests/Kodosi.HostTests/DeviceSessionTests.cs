using Kodosi.Security;
using Xunit;

namespace Kodosi.HostTests;

public sealed class DeviceSessionTests
{
    [Fact]
    public void AChallengeServesOnceAndOnlyItsAccount()
    {
        var sessions = new DeviceSessions(TimeProvider.System);
        var user = Guid.CreateVersion7(); var other = Guid.CreateVersion7();
        var (taken, _) = sessions.NewChallenge(user);
        Assert.Equal(403, Assert.Throws<ApiException>(() => sessions.ConsumeChallenge(other, taken)).Status);
        Assert.Equal(403, Assert.Throws<ApiException>(() => sessions.ConsumeChallenge(user, taken)).Status);
        var (id, bytes) = sessions.NewChallenge(user);
        Assert.Equal(bytes, sessions.ConsumeChallenge(user, id));
        Assert.Equal(403, Assert.Throws<ApiException>(() => sessions.ConsumeChallenge(user, id)).Status);
        for (var index = 0; index < 16; index++) sessions.NewChallenge(user);
        Assert.Equal(429, Assert.Throws<ApiException>(() => sessions.NewChallenge(user)).Status);
        sessions.NewChallenge(other);
    }

    [Fact]
    public void ASessionHoldsForItsDeviceUntilItExpiresOrTheDeviceGoes()
    {
        var clock = new ManualClock();
        var sessions = new DeviceSessions(clock);
        var user = Guid.CreateVersion7();
        var first = sessions.Open(user, "phone");
        Assert.True(sessions.Holds(user, "phone", first));
        Assert.False(sessions.Holds(user, "mac", first));
        Assert.False(sessions.Holds(Guid.CreateVersion7(), "phone", first));
        Assert.False(sessions.Holds(user, "phone", first[..^2] + "AA"));
        Assert.False(sessions.Holds(user, "phone", new string('A', 65)));
        Assert.False(sessions.Holds(user, "phone", null));

        var newer = Enumerable.Range(0, 4).Select(_ => sessions.Open(user, "phone")).ToArray();
        Assert.False(sessions.Holds(user, "phone", first));
        Assert.All(newer, secret => Assert.True(sessions.Holds(user, "phone", secret)));
        var mac = sessions.Open(user, "mac");
        sessions.Remove(user, "phone");
        Assert.All(newer, secret => Assert.False(sessions.Holds(user, "phone", secret)));
        Assert.True(sessions.Holds(user, "mac", mac));

        clock.Advance(TimeSpan.FromHours(12));
        Assert.False(sessions.Holds(user, "mac", mac));
        var fresh = sessions.Open(Guid.CreateVersion7(), "other");
        Assert.Equal(1, sessions.Count);
        Assert.NotEqual(fresh, mac);
    }
}
