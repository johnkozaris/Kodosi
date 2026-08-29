using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class DeviceHttpRequestProofPreimageTests
{
    private static readonly UserId User =
        UserId.From(Guid.Parse("01900000-0000-7000-8000-000000000001"));
    private static readonly Guid ChallengeId =
        Guid.Parse("01900000-0000-7000-8000-000000000002");
    private const string EmptyHash =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    [Fact]
    public void Request_Proof_Binds_Nonce_Method_Path_Query_Body_And_Device()
    {
        var nonce = Enumerable.Range(0, 32).Select(static value => (byte)value).ToArray();
        var canonical = Create(nonce);

        Assert.NotEqual(canonical, Create(nonce, device: "device-b"));
        Assert.NotEqual(canonical, Create(nonce, method: "POST"));
        Assert.NotEqual(canonical, Create(nonce, path: "/api/me/semantic-receipts?limit=64"));
        Assert.NotEqual(canonical, Create(nonce, bodyHash: new string('a', 64)));
        Assert.NotEqual(canonical, Create(nonce, challengeId: Guid.NewGuid()));
        Assert.NotEqual(canonical, Create([.. nonce[..31], (byte)99]));
    }

    [Fact]
    public void Request_Proof_Rejects_Malformed_Tuples()
    {
        Assert.Throws<DomainException>(() => Create(new byte[31]));
        Assert.Throws<DomainException>(() => Create(new byte[32], device: ""));
        Assert.Throws<DomainException>(() => Create(new byte[32], challengeId: Guid.Empty));
        Assert.Throws<DomainException>(() => Create(new byte[32], bodyHash: "bad"));
    }

    private static byte[] Create(
        byte[] nonce,
        string device = "device-a",
        Guid? challengeId = null,
        string method = "GET",
        string path = "/api/me/semantic-receipts?limit=32",
        string bodyHash = EmptyHash) =>
        DeviceHttpRequestProofPreimage.Create(
            User,
            device,
            challengeId ?? ChallengeId,
            method,
            path,
            bodyHash,
            nonce);
}
