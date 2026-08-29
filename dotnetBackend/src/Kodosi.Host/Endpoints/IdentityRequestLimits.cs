using Kodosi.Domain;

namespace Kodosi.Host.Endpoints;

internal static class IdentityRequestLimits
{
    private static long Base64Length(long bytes) => ((bytes + 2) / 3) * 4;



    public static readonly long MutationBodyBytes =
        Base64Length(IdentityWireFormat.MaxSignedDeviceListBodyLength)
        + Base64Length(IdentityWireFormat.MaxDeviceCertificateBodyLength)
        + (2 * Base64Length(IdentityWireFormat.MlDsa65SignatureLength))
        + Base64Length(IdentityWireFormat.MlKem768PublicKeyLength)
        + Base64Length(IdentityWireFormat.MlDsa65PublicKeyLength)
        + (128 * 1024);
}
