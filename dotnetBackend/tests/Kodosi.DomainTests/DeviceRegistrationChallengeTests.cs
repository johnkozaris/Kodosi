using Kodosi.Domain;

namespace Kodosi.DomainTests;

public class DeviceRegistrationChallengeTests
{
    [Fact]
    public void Create_WithWrongChallengeLength_Throws()
    {
        Assert.Throws<DomainException>(() =>
            DeviceRegistrationChallenge.Create(
                UserId.New(),
                new byte[16],
                TimeSpan.FromMinutes(5)));
    }

    [Fact]
    public void Create_WithNonPositiveTtl_Throws()
    {
        Assert.Throws<DomainException>(() =>
            DeviceRegistrationChallenge.Create(
                UserId.New(),
                new byte[32],
                TimeSpan.Zero));
    }

    [Fact]
    public void IsValid_ReturnsFalseAtExpiry()
    {
        var challenge = DeviceRegistrationChallenge.Create(
            UserId.New(),
            new byte[32],
            TimeSpan.FromMinutes(5));

        Assert.True(challenge.IsValid(challenge.ExpiresAt.AddTicks(-1)));
        Assert.False(challenge.IsValid(challenge.ExpiresAt));
    }

    [Fact]
    public void ChallengeBytes_AreCopied_NotShared()
    {
        var original = new byte[32];
        original[0] = 0xAA;

        var challenge = DeviceRegistrationChallenge.Create(
            UserId.New(),
            original,
            TimeSpan.FromMinutes(5));

        original[0] = 0x00;
        Assert.Equal(0xAA, challenge.Challenge[0]);
    }

}
