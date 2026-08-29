namespace Kodosi.Domain;

public static class IdentityWireFormat
{
    public const uint MaxFieldLength = 65_536;

    public const uint MaxEntries = 256;

    public const ulong NoExpirySentinel = 0;

    public const long MaxUnixTimeMilliseconds = 253_402_300_799_999;

    public const int MaxDeviceCertificateBodyLength = 5_772;

    public const int MaxSignedDeviceListBodyLength = 527_432;

    public const int UserIdLength = 36;

    public const int DeviceIdMaxUtf16CodeUnits = 256;

    public const int DeviceLabelMaxUtf16CodeUnits = 128;

    public const int MlKem768PublicKeyLength = 1_184;

    public const int MlDsa65PublicKeyLength = 1_952;

    public const int MlDsa65SignatureLength = 3_309;
}
