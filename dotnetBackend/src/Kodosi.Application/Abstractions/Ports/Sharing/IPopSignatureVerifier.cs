namespace Kodosi.Application;



public interface IPopSignatureVerifier
{
    bool Verify(
        ReadOnlySpan<byte> publicKey,
        ReadOnlySpan<byte> message,
        ReadOnlySpan<byte> signature);
}
