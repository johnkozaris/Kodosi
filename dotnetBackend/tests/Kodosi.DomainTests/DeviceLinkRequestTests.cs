using Kodosi.Domain;

namespace Kodosi.DomainTests;

public class DeviceLinkRequestTests
{
    private const int KemPublicKeyLength = 1184;
    private const int SigningPublicKeyLength = 1952;

    private static DeviceLinkRequest BuildPending(string userCode = "ABCD-EFGH")
    {
        return DeviceLinkRequest.Create(
            UserId.New(),
            deviceCode: "secret-device-code",
            userCode: userCode,
            deviceId: "user-1-aaaa-bbbb-cccc",
            deviceLabel: "mbp",
            kemPublicKey: new byte[KemPublicKeyLength],
            signingPublicKey: new byte[SigningPublicKeyLength],
            ttl: TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
    }

    [Fact]
    public void Create_WithEmptyDeviceId_Throws()
    {
        Assert.Throws<DomainException>(() =>
            DeviceLinkRequest.Create(
                UserId.New(),
                "dc", "uc", deviceId: "  ", deviceLabel: "",
                new byte[KemPublicKeyLength],
                new byte[SigningPublicKeyLength],
                TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow));
    }

    [Fact]
    public void Create_EnforcesDeviceIdPersistenceBound()
    {
        var accepted = DeviceLinkRequest.Create(
            UserId.New(),
            "dc", "uc", $"  {new string('d', DeviceIdRules.MaximumLength)}  ", "label",
            new byte[KemPublicKeyLength],
            new byte[SigningPublicKeyLength],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
        Assert.Equal(DeviceIdRules.MaximumLength, accepted.DeviceId.Length);

        Assert.Throws<DomainException>(() => DeviceLinkRequest.Create(
            UserId.New(),
            "dc", "uc", new string('d', DeviceIdRules.MaximumLength + 1), "label",
            new byte[KemPublicKeyLength],
            new byte[SigningPublicKeyLength],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow));
    }

    [Fact]
    public void Create_WithWrongKemKeyLength_Throws()
    {
        Assert.Throws<DomainException>(() =>
            DeviceLinkRequest.Create(
                UserId.New(),
                "dc", "uc", "dev", "label",
                new byte[8],
                new byte[SigningPublicKeyLength],
                TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow));
    }

    [Fact]
    public void Create_TruncatesLabelAt128Chars()
    {
        var longLabel = new string('x', 300);
        var request = DeviceLinkRequest.Create(
            UserId.New(),
            "dc", "uc", "dev", longLabel,
            new byte[KemPublicKeyLength],
            new byte[SigningPublicKeyLength],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
        Assert.Equal(128, request.DeviceLabel.Length);
    }

    [Fact]
    public void Create_TrimsLabelWhitespace()
    {
        var request = DeviceLinkRequest.Create(
            UserId.New(),
            "dc", "uc", "dev", "   my laptop   ",
            new byte[KemPublicKeyLength],
            new byte[SigningPublicKeyLength],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
        Assert.Equal("my laptop", request.DeviceLabel);
    }

    [Fact]
    public void IsPending_TrueImmediatelyAfterCreate()
    {
        var request = BuildPending();
        Assert.True(request.IsPending(DateTimeOffset.UtcNow));
    }

    [Fact]
    public void IsPending_FalseAfterApprove()
    {
        var request = BuildPending();
        request.Approve(2, DateTimeOffset.UtcNow);
        Assert.False(request.IsPending(DateTimeOffset.UtcNow));
    }

    [Fact]
    public void IsPending_FalseAfterCancel()
    {
        var request = BuildPending();
        DomainFixtureHydrator.CancelDeviceLink(request, DateTimeOffset.UtcNow);
        Assert.False(request.IsPending(DateTimeOffset.UtcNow));
    }

    [Fact]
    public void IsPending_FalsePastExpiry()
    {
        var request = BuildPending();
        Assert.False(request.IsPending(request.ExpiresAt.AddSeconds(1)));
    }

    [Fact]
    public void Approve_TwiceThrows()
    {
        var request = BuildPending();
        request.Approve(2, DateTimeOffset.UtcNow);
        Assert.Throws<DomainException>(() =>
            request.Approve(2, DateTimeOffset.UtcNow));
    }

    [Fact]
    public void Approve_AfterCancelThrows()
    {
        var request = BuildPending();
        DomainFixtureHydrator.CancelDeviceLink(request, DateTimeOffset.UtcNow);
        Assert.Throws<DomainException>(() =>
            request.Approve(2, DateTimeOffset.UtcNow));
    }

}
