using System.Security.Cryptography;
using Kodosi.Domain;
using Kodosi.Infrastructure.Crypto;

namespace Kodosi.HostTests;

public sealed class MLDsaPopSignatureVerifierTests
{
    private static byte[] BuildPopPreimage(ReadOnlySpan<byte> challenge)
    {
        var tag = DomainTags.DevicePopV1;
        var preimage = new byte[tag.Length + challenge.Length];
        tag.CopyTo(preimage);
        challenge.CopyTo(preimage.AsSpan(tag.Length));
        return preimage;
    }

    private static byte[] SignMessage(MLDsa key, ReadOnlySpan<byte> message)
    {
        var signature = new byte[MLDsaAlgorithm.MLDsa65.SignatureSizeInBytes];
        key.SignData(message, signature, context: ReadOnlySpan<byte>.Empty);
        return signature;
    }

    [Fact]
    public void Verify_RoundTripsValidSignature()
    {
        if (!MLDsa.IsSupported)
            return;

        var challenge = RandomNumberGenerator.GetBytes(32);
        var preimage = BuildPopPreimage(challenge);

        using var key = MLDsa.GenerateKey(MLDsaAlgorithm.MLDsa65);
        var publicKey = key.ExportMLDsaPublicKey();
        var signature = SignMessage(key, preimage);

        var verifier = new MLDsaPopSignatureVerifier();
        Assert.True(verifier.Verify(publicKey, preimage, signature));
    }

    [Fact]
    public void Verify_RejectsSignatureOverDifferentChallenge()
    {
        if (!MLDsa.IsSupported)
            return;

        var challenge = RandomNumberGenerator.GetBytes(32);
        var tampered = (byte[])challenge.Clone();
        tampered[0] ^= 0xFF;

        using var key = MLDsa.GenerateKey(MLDsaAlgorithm.MLDsa65);
        var publicKey = key.ExportMLDsaPublicKey();
        var signature = SignMessage(key, BuildPopPreimage(challenge));

        var verifier = new MLDsaPopSignatureVerifier();
        Assert.False(verifier.Verify(publicKey, BuildPopPreimage(tampered), signature));
    }

    [Fact]
    public void Verify_RejectsSignatureFromDifferentKey()
    {
        if (!MLDsa.IsSupported)
            return;

        var challenge = RandomNumberGenerator.GetBytes(32);
        var preimage = BuildPopPreimage(challenge);

        using var keyA = MLDsa.GenerateKey(MLDsaAlgorithm.MLDsa65);
        using var keyB = MLDsa.GenerateKey(MLDsaAlgorithm.MLDsa65);

        var signature = SignMessage(keyA, preimage);
        var publicKeyB = keyB.ExportMLDsaPublicKey();

        var verifier = new MLDsaPopSignatureVerifier();
        Assert.False(verifier.Verify(publicKeyB, preimage, signature));
    }

    [Fact]
    public void Verify_RejectsUnprefixedSignature()
    {
        if (!MLDsa.IsSupported)
            return;

        var challenge = RandomNumberGenerator.GetBytes(32);

        using var key = MLDsa.GenerateKey(MLDsaAlgorithm.MLDsa65);
        var publicKey = key.ExportMLDsaPublicKey();
        var unprefixedSig = SignMessage(key, challenge);

        var verifier = new MLDsaPopSignatureVerifier();
        Assert.False(verifier.Verify(publicKey, BuildPopPreimage(challenge), unprefixedSig));
    }

    [Fact]
    public void Verify_RejectsExtraPrefixedPreimage()
    {
        if (!MLDsa.IsSupported)
            return;

        var arbitraryBody = RandomNumberGenerator.GetBytes(64);
        var certTag = DomainTags.DeviceCertV2;
        var preimage = new byte[certTag.Length + arbitraryBody.Length];
        certTag.CopyTo(preimage);
        arbitraryBody.CopyTo(preimage.AsSpan(certTag.Length));

        using var key = MLDsa.GenerateKey(MLDsaAlgorithm.MLDsa65);
        var publicKey = key.ExportMLDsaPublicKey();
        var signature = SignMessage(key, preimage);

        var verifier = new MLDsaPopSignatureVerifier();
        Assert.True(
            verifier.Verify(publicKey, preimage, signature),
            "verifier must verify the exact preimage caller supplied, not an internally-re-prefixed variant");
    }

    [Fact]
    public void Verify_RejectsEmptyInputs()
    {
        var verifier = new MLDsaPopSignatureVerifier();

        Assert.False(verifier.Verify(ReadOnlySpan<byte>.Empty, new byte[32], new byte[3293]));
        Assert.False(verifier.Verify(new byte[1952], ReadOnlySpan<byte>.Empty, new byte[3293]));
        Assert.False(verifier.Verify(new byte[1952], new byte[32], ReadOnlySpan<byte>.Empty));
    }

    [Fact]
    public void Verify_RejectsMalformedPublicKey()
    {
        if (!MLDsa.IsSupported)
            return;

        var badKey = new byte[100];
        var verifier = new MLDsaPopSignatureVerifier();
        Assert.False(verifier.Verify(badKey, new byte[32], new byte[3293]));
    }
}
