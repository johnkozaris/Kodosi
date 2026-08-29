using Kodosi.Domain;

namespace Kodosi.DomainTests;

public class UserDeviceCertificateTests
{
    private static readonly DateTimeOffset IssuedAt =
        DateTimeOffset.FromUnixTimeMilliseconds(1_700_000_000_000);
    private static readonly DateTimeOffset ValidatedAt = IssuedAt.AddMinutes(1);

    [Fact]
    public void CreateCertified_DerivesCertificateFieldsFromExactBody()
    {
        var userId = UserId.New();
        var deviceId = "device-1";
        var certificate = Certificate(
            userId,
            deviceId,
            deviceLabel: "Alice's MacBook",
            signerDeviceId: "signer-device",
            expiresAt: IssuedAt.AddDays(35));

        var device = UserDevice.CreateCertified(
            userId,
            deviceId,
            certificate,
            Signature(),
            ValidatedAt);

        Assert.Equal("Alice's MacBook", device.DeviceLabel);
        Assert.Equal("signer-device", device.CertSignerDeviceId);
        Assert.Equal(IssuedAt, device.CertIssuedAt);
        Assert.Equal(IssuedAt.AddDays(35), device.CertExpiresAt);
        Assert.Equal(1184, device.KemPublicKey.Length);
        Assert.Equal(1952, device.SigningPublicKey.Length);
    }

    [Fact]
    public void CreateCertified_RequiresCertificateIdentityToMatchPersistenceKey()
    {
        var userId = UserId.New();

        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            "device-2",
            Certificate(userId, "device-1"),
            Signature(),
            ValidatedAt));
        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            "device-1",
            Certificate(UserId.New(), "device-1"),
            Signature(),
            ValidatedAt));
    }

    [Fact]
    public void CreateCertified_EnforcesDeviceIdPersistenceBound()
    {
        var userId = UserId.New();
        var acceptedId = new string('d', DeviceIdRules.MaximumLength);
        var accepted = UserDevice.CreateCertified(
            userId,
            acceptedId,
            Certificate(userId, acceptedId),
            Signature(),
            ValidatedAt);

        Assert.Equal(DeviceIdRules.MaximumLength, accepted.DeviceId.Length);
        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            new string('d', DeviceIdRules.MaximumLength + 1),
            Certificate(userId, acceptedId),
            Signature(),
            ValidatedAt));
        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            acceptedId,
            Certificate(
                userId,
                acceptedId,
                signerDeviceId: new string('s', DeviceIdRules.MaximumLength + 1)),
            Signature(),
            ValidatedAt));
        Assert.Throws<DomainException>(() => accepted.Revoke(
            new string('r', DeviceIdRules.MaximumLength + 1)));
    }

    [Theory]
    [InlineData(0, 1952)]
    [InlineData(1183, 1952)]
    [InlineData(1184, 1951)]
    public void CreateCertified_RejectsInvalidPublicKeyLengths(int kemLength, int signingLength)
    {
        var userId = UserId.New();

        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            "device-1",
            Certificate(
                userId,
                "device-1",
                kemPublicKey: new byte[kemLength],
                signingPublicKey: new byte[signingLength]),
            Signature(),
            ValidatedAt));
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData(" Laptop")]
    [InlineData("Laptop ")]
    public void CreateCertified_RejectsNoncanonicalLabel(string label)
    {
        var userId = UserId.New();

        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            "device-1",
            Certificate(userId, "device-1", deviceLabel: label),
            Signature(),
            ValidatedAt));
    }

    [Fact]
    public void CreateCertified_RejectsInvalidExpiry()
    {
        var userId = UserId.New();

        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            "device-1",
            Certificate(userId, "device-1", expiresAt: IssuedAt),
            Signature(),
            ValidatedAt));
        var exception = Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            "device-1",
            Certificate(userId, "device-1", expiresAt: ValidatedAt),
            Signature(),
            ValidatedAt));
        Assert.Contains("past", exception.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void CreateCertified_RequiresBoundedBodyAndExactSignatureLength()
    {
        var userId = UserId.New();
        var certificate = Certificate(userId, "device-1");

        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            "device-1",
            [],
            Signature(),
            ValidatedAt));
        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            "device-1",
            new byte[IdentityWireFormat.MaxDeviceCertificateBodyLength + 1],
            Signature(),
            ValidatedAt));
        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            "device-1",
            certificate,
            new byte[IdentityWireFormat.MlDsa65SignatureLength - 1],
            ValidatedAt));
    }

    [Fact]
    public void CreateCertified_ClonesProofBytes()
    {
        var userId = UserId.New();
        var certificate = Certificate(userId, "device-1");
        var signature = Signature();
        var expectedCertificate = certificate.ToArray();
        var expectedSignature = signature.ToArray();

        var device = UserDevice.CreateCertified(
            userId,
            "device-1",
            certificate,
            signature,
            ValidatedAt);
        certificate[0] ^= 0xFF;
        signature[0] ^= 0xFF;

        Assert.Equal(expectedCertificate, device.DeviceCertificate);
        Assert.Equal(expectedSignature, device.DeviceCertificateSignature);
        Assert.True(device.MatchesEnrollment(
            userId,
            "device-1",
            new byte[1184],
            new byte[1952],
            expectedCertificate,
            expectedSignature));
    }

    [Fact]
    public void ReturnedProofAndKeyBytesCannotMutateAuthority()
    {
        var userId = UserId.New();
        var certificate = Certificate(
            userId,
            "device-1",
            kemPublicKey: Enumerable.Repeat((byte)1, 1184).ToArray(),
            signingPublicKey: Enumerable.Repeat((byte)2, 1952).ToArray());
        var signature = Signature();
        var device = UserDevice.CreateCertified(
            userId,
            "device-1",
            certificate,
            signature,
            ValidatedAt);

        var returnedCertificate = device.DeviceCertificate;
        var returnedSignature = device.DeviceCertificateSignature;
        var returnedKemKey = device.KemPublicKey;
        var returnedSigningKey = device.SigningPublicKey;
        returnedCertificate[0] ^= 0xFF;
        returnedSignature[0] ^= 0xFF;
        returnedKemKey[0] ^= 0xFF;
        returnedSigningKey[0] ^= 0xFF;

        Assert.Equal(certificate, device.DeviceCertificate);
        Assert.Equal(signature, device.DeviceCertificateSignature);
        Assert.Equal(1, device.KemPublicKey[0]);
        Assert.Equal(2, device.SigningPublicKey[0]);
        Assert.True(device.MatchesEnrollment(
            userId,
            "device-1",
            Enumerable.Repeat((byte)1, 1184).ToArray(),
            Enumerable.Repeat((byte)2, 1952).ToArray(),
            certificate,
            signature));
    }

    [Fact]
    public void CreateCertified_Rejects_Noncanonical_User_Id_Text()
    {
        var userId = UserId.New();
        var body = IdentityWireTestBody.DeviceCertificate(
            userId.Value,
            userIdText: userId.Value.ToString("D").ToUpperInvariant());

        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            "d1",
            body,
            Signature(),
            ValidatedAt));
    }

    [Fact]
    public void DeviceIds_Use_Utf16_Code_Unit_Bound()
    {
        var userId = UserId.New();
        var acceptedId = string.Concat(Enumerable.Repeat("😀", DeviceIdRules.MaximumLength / 2));
        var accepted = UserDevice.CreateCertified(
            userId,
            acceptedId,
            Certificate(userId, acceptedId),
            Signature(),
            ValidatedAt);

        Assert.Equal(DeviceIdRules.MaximumLength, accepted.DeviceId.Length);
        var rejectedId = acceptedId + "a";
        Assert.Throws<DomainException>(() => UserDevice.CreateCertified(
            userId,
            rejectedId,
            Certificate(userId, acceptedId),
            Signature(),
            ValidatedAt));
    }

    [Fact]
    public void Revoke_IsIdempotentAndPreservesFirstRevoker()
    {
        var userId = UserId.New();
        var device = UserDevice.CreateCertified(
            userId,
            "device-1",
            Certificate(userId, "device-1"),
            Signature(),
            ValidatedAt);

        device.Revoke("revoker-1");
        var first = device.RevokedAt;
        device.Revoke("revoker-2");

        Assert.Equal(first, device.RevokedAt);
        Assert.Equal("revoker-1", device.RevokedByDeviceId);
    }

    private static byte[] Certificate(
        UserId userId,
        string deviceId,
        string deviceLabel = "Test device",
        string? signerDeviceId = null,
        byte[]? kemPublicKey = null,
        byte[]? signingPublicKey = null,
        DateTimeOffset? expiresAt = null) =>
        IdentityWireTestBody.DeviceCertificate(
            userId.Value,
            deviceId,
            deviceLabel,
            signerDeviceId ?? deviceId,
            kemPublicKey,
            signingPublicKey,
            IssuedAt.ToUnixTimeMilliseconds(),
            expiresAt?.ToUnixTimeMilliseconds());

    private static byte[] Signature() =>
        new byte[IdentityWireFormat.MlDsa65SignatureLength];
}
