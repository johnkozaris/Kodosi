using System.Security.Cryptography;
using Kodosi.Application;
using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Crypto.Signers;

namespace Kodosi.Infrastructure.Crypto;

public sealed class MLDsaPopSignatureVerifier : IPopSignatureVerifier
{
    public bool Verify(
        ReadOnlySpan<byte> publicKey,
        ReadOnlySpan<byte> message,
        ReadOnlySpan<byte> signature)
    {
        if (publicKey.IsEmpty || message.IsEmpty || signature.IsEmpty)
        {
            return false;
        }

        if (MLDsa.IsSupported)
        {
            try
            {
                using var mldsa = MLDsa.ImportMLDsaPublicKey(MLDsaAlgorithm.MLDsa65, publicKey);
                return mldsa.VerifyData(message, signature, context: ReadOnlySpan<byte>.Empty);
            }
            catch (CryptographicException)
            {
                return false;
            }
        }

        return VerifyWithBouncyCastle(publicKey, message, signature);
    }

    private static bool VerifyWithBouncyCastle(
        ReadOnlySpan<byte> publicKey,
        ReadOnlySpan<byte> message,
        ReadOnlySpan<byte> signature)
    {
        try
        {
            var pubParams = MLDsaPublicKeyParameters.FromEncoding(
                MLDsaParameters.ml_dsa_65, publicKey.ToArray());
            var signer = new MLDsaSigner(MLDsaParameters.ml_dsa_65, deterministic: false);
            signer.Init(forSigning: false, pubParams);
            signer.BlockUpdate(message);
            return signer.VerifySignature(signature.ToArray());
        }
        catch (ArgumentException)
        {
            return false;
        }
        catch (InvalidOperationException)
        {
            return false;
        }
    }
}
