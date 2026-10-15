using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Crypto.Signers;

namespace Kodosi.Security;

internal static class SignatureVerifier
{
    public static bool Verify(ReadOnlySpan<byte> publicKey, ReadOnlySpan<byte> message, ReadOnlySpan<byte> signature)
    {
        if (publicKey.Length != IdentityWireFormat.MlDsa65PublicKeyLength || message.IsEmpty
            || signature.Length != IdentityWireFormat.MlDsa65SignatureLength)
            return false;
        try
        {
            var signer = new MLDsaSigner(MLDsaParameters.ml_dsa_65, deterministic: false);
            signer.Init(forSigning: false, MLDsaPublicKeyParameters.FromEncoding(MLDsaParameters.ml_dsa_65, publicKey.ToArray()));
            signer.BlockUpdate(message);
            return signer.VerifySignature(signature.ToArray());
        }
        catch (Exception error) when (error is ArgumentException or InvalidOperationException)
        {
            return false;
        }
    }
}
