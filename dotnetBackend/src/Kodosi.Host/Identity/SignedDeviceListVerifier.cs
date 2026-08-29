using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Crypto;

namespace Kodosi.Host.Identity;

internal sealed class SignedDeviceListVerifier(
    SignedDeviceListParser parser,
    IPopSignatureVerifier signatures) : ISignedDeviceListVerifier
{
    public SignedDeviceListParser.ParsedSignedDeviceList Parse(ReadOnlySpan<byte> body)
    {
        try
        {
            return parser.Parse(body);
        }
        catch (SignedDeviceListFormatException exception)
        {
            throw new DeviceEnrollmentException(
                $"Invalid signed device list: {exception.Message}");
        }
    }

    public bool Verify(
        ReadOnlySpan<byte> body,
        ReadOnlySpan<byte> signature,
        ReadOnlySpan<byte> signerPublicKey)
    {
        var payload = new byte[SignedDeviceListParser.DomainTag.Length + body.Length];
        SignedDeviceListParser.DomainTag.CopyTo(payload);
        body.CopyTo(payload.AsSpan(SignedDeviceListParser.DomainTag.Length));
        return signatures.Verify(signerPublicKey, payload, signature);
    }
}
