using System.Security.Cryptography;
using Kodosi.Domain;
using Kodosi.Infrastructure.Crypto;

namespace Kodosi.HostTests;

public sealed class SessionKeyBlobTests
{
    [Fact]
    public void SignatureDigest_MatchesRustProducedParityVector()
    {
        var digest = SessionKeyBlobSignatureDigest.ComputeV1(
            "sess",
            "dev",
            [1, 2, 3],
            keyGeneration: 1,
            issuedAtMs: 0);

        Assert.Equal(
            "78039b5ba8ecf76935f30c908c1538376209c334b1a1625f240e9fb8188ae4d5",
            Convert.ToHexStringLower(digest));
    }

    [Fact]
    public void SignatureDigestV2_BindsCanonicalIncarnationParityVector()
    {
        var digest = SessionKeyBlobSignatureDigest.ComputeV2(
            "sess",
            Guid.Parse("01900000-0000-7000-8000-000000000002"),
            "dev",
            [1, 2, 3],
            keyGeneration: 1,
            issuedAtMs: 0);

        Assert.Equal(
            "f8cdc6ee91d6d3ea2502a71d3012e255c1b8ca232de151788530284fb4d4efdc",
            Convert.ToHexStringLower(digest));
    }

    [Fact]
    public void SignatureDigest_VerifiesValidSignatureAndRejectsTamperedPayload()
    {
        if (!MLDsa.IsSupported)
        {
            return;
        }

        using var signingKey = MLDsa.GenerateKey(MLDsaAlgorithm.MLDsa65);
        var publicKey = signingKey.ExportMLDsaPublicKey();
        var incarnationId = Guid.Parse("01900000-0000-7000-8000-000000000002");
        var digest = SessionKeyBlobSignatureDigest.ComputeV2(
            "8f96b97c-42e7-454d-a250-a5580014b1dc",
            incarnationId,
            "viewer-device",
            [1, 2, 3],
            keyGeneration: 7,
            issuedAtMs: 1_786_000_000_000);
        var signature = new byte[MLDsaAlgorithm.MLDsa65.SignatureSizeInBytes];
        signingKey.SignData(digest, signature, context: ReadOnlySpan<byte>.Empty);
        var verifier = new MLDsaPopSignatureVerifier();

        Assert.True(verifier.Verify(publicKey, digest, signature));
        Assert.False(verifier.Verify(
            publicKey,
            SessionKeyBlobSignatureDigest.ComputeV2(
                "8f96b97c-42e7-454d-a250-a5580014b1dc",
                incarnationId,
                "viewer-device",
                [1, 2, 4],
                keyGeneration: 7,
                issuedAtMs: 1_786_000_000_000),
            signature));
    }

    [Fact]
    public void Create_Rejects_Empty_Signature()
    {
        var sessionId = SessionId.New();

        var error = Assert.Throws<DomainException>(() => SessionKeyBlob.Create(
            sessionId,
            "recipient-device",
            [1, 2, 3],
            "sender-device",
            [4, 5, 6],
            1,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [],
            issuedAtMs: 1_000L));

        Assert.Equal("Key blob signature is required.", error.Message);
    }

    [Fact]
    public void Create_Rejects_Negative_KeyGeneration()
    {
        var error = Assert.Throws<DomainException>(() => SessionKeyBlob.Create(
            SessionId.New(),
            "recipient-device",
            [1, 2, 3],
            "sender-device",
            [4, 5, 6],
            -1,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [10, 11, 12],
            issuedAtMs: 1_000L));

        Assert.Equal("Key generation must be non-negative.", error.Message);
    }

    [Theory]
    [InlineData(0L)]
    [InlineData(-1L)]
    public void Create_Rejects_NonPositive_IssuedAtMs(long issuedAtMs)
    {


        var error = Assert.Throws<DomainException>(() => SessionKeyBlob.Create(
            SessionId.New(),
            "recipient-device",
            [1, 2, 3],
            "sender-device",
            [4, 5, 6],
            1,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [10, 11, 12],
            issuedAtMs: issuedAtMs));

        Assert.Equal(
            "IssuedAtMs must be a positive Unix epoch milliseconds value.",
            error.Message);
    }
}
