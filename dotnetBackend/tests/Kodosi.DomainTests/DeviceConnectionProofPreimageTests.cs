using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class DeviceConnectionProofPreimageTests
{
    private static readonly UserId User =
        UserId.From(Guid.Parse("01900000-0000-7000-8000-000000000001"));
    private static readonly Guid Session =
        Guid.Parse("01900000-0000-7000-8000-000000000002");
    private static readonly Guid Incarnation =
        Guid.Parse("01900000-0000-7000-8000-000000000003");

    [Fact]
    public void Preimage_Binds_Every_Connection_Context_Field()
    {
        var challenge = Enumerable.Range(0, 32).Select(static value => (byte)value).ToArray();
        var canonical = DeviceConnectionProofPreimage.Create(
            User,
            "device-a",
            "connection-a",
            "host",
            Session.ToString("D"),
            Incarnation,
            challenge);

        Assert.NotEqual(canonical, Create(challenge, device: "device-b"));
        Assert.NotEqual(canonical, Create(challenge, connection: "connection-b"));
        Assert.NotEqual(canonical, Create(challenge, purpose: "participant"));
        Assert.NotEqual(canonical, Create(challenge, session: Guid.NewGuid().ToString("D")));
        Assert.NotEqual(canonical, Create(challenge, incarnation: Guid.NewGuid()));
        Assert.NotEqual(canonical, Create([.. challenge[..31], (byte)99]));
    }

    [Fact]
    public void Preimage_Rejects_Incomplete_Identity_And_Challenge()
    {
        Assert.Throws<DomainException>(() => Create(new byte[31]));
        Assert.Throws<DomainException>(() => Create(new byte[32], device: " "));
        Assert.Throws<DomainException>(() => Create(new byte[32], connection: ""));
        Assert.Throws<DomainException>(() => Create(new byte[32], purpose: ""));
    }

    private static byte[] Create(
        byte[] challenge,
        string device = "device-a",
        string connection = "connection-a",
        string purpose = "host",
        string? session = null,
        Guid? incarnation = null) =>
        DeviceConnectionProofPreimage.Create(
            User,
            device,
            connection,
            purpose,
            session ?? Session.ToString("D"),
            incarnation ?? Incarnation,
            challenge);
}
