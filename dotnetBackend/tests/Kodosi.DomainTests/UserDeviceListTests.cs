using Kodosi.Domain;

namespace Kodosi.DomainTests;

public class UserDeviceListTests
{
    [Fact]
    public void Create_Parses_Exact_Body_As_The_Sole_Semantic_Authority()
    {
        var userId = UserId.New();
        var body = IdentityWireTestBody.DeviceList(
            userId.Value,
            generation: 7,
            deviceId: "device-1",
            signerDeviceId: "device-1",
            issuedAtMs: 1_700_000_000_000,
            expiresAtMs: 1_700_000_001_000);

        var list = UserDeviceList.Create(userId, 7, body, Signature());
        var proof = list.ParseBody();

        Assert.Equal(7, list.Generation);
        Assert.Equal("device-1", proof.SignerDeviceId);
        Assert.Equal(1_700_000_000_000, proof.IssuedAtMs);
        Assert.Equal(1_700_000_001_000, proof.ExpiresAtMs);
        Assert.Equal(["device-1"], proof.Entries.Select(entry => entry.DeviceId));
    }

    [Fact]
    public void Create_Rejects_Body_Key_Or_Owner_Divergence()
    {
        var owner = UserId.New();
        var other = UserId.New();
        var body = IdentityWireTestBody.DeviceList(other.Value, generation: 2);

        Assert.Throws<DomainException>(() =>
            UserDeviceList.Create(owner, 2, body, Signature()));
        Assert.Throws<DomainException>(() =>
            UserDeviceList.Create(other, 3, body, Signature()));
    }

    [Fact]
    public void Proof_Bytes_Are_Defensive_On_Input_And_Output()
    {
        var userId = UserId.New();
        var body = IdentityWireTestBody.DeviceList(userId.Value);
        var signature = Signature();
        var expectedBody = body.ToArray();
        var expectedSignature = signature.ToArray();
        var list = UserDeviceList.Create(userId, 1, body, signature);

        body[0] ^= 1;
        signature[0] ^= 1;
        var returnedBody = list.Body;
        var returnedSignature = list.Signature;
        returnedBody[0] ^= 1;
        returnedSignature[0] ^= 1;

        Assert.Equal(expectedBody, list.Body);
        Assert.Equal(expectedSignature, list.Signature);
        Assert.True(list.MatchesEnrollment(userId, expectedBody, expectedSignature));
    }

    [Theory]
    [InlineData(true, false)]
    [InlineData(false, true)]
    public void Create_Rejects_Invalid_Proof_Length(bool invalidBody, bool invalidSignature)
    {
        var userId = UserId.New();
        var body = invalidBody
            ? new byte[IdentityWireFormat.MaxSignedDeviceListBodyLength + 1]
            : IdentityWireTestBody.DeviceList(userId.Value);
        var signature = invalidSignature
            ? new byte[IdentityWireFormat.MlDsa65SignatureLength - 1]
            : Signature();

        Assert.Throws<DomainException>(() =>
            UserDeviceList.Create(userId, 1, body, signature));
    }

    [Fact]
    public void DeviceListGenerationCollisionException_Uses_Dedicated_Code()
    {
        var exception = new DeviceListGenerationCollisionException();
        Assert.Equal("DEVICE_LIST_GENERATION_COLLISION", exception.Code);
    }

    private static byte[] Signature() =>
        new byte[IdentityWireFormat.MlDsa65SignatureLength];
}
