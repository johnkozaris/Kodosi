
namespace Kodosi.Security;


public sealed class DeviceCertificateParser
{
    public static ReadOnlySpan<byte> DomainTag => DomainTags.DeviceCertV2;

    private const ulong NoExpiry = IdentityWireFormat.NoExpirySentinel;

    public sealed record ParsedDeviceCertificate(
        string UserId,
        string DeviceId,
        string DeviceLabel,
        string SignerDeviceId,
        byte[] KemPublicKey,
        byte[] SigPublicKey,
        long IssuedAtMs,
        long? ExpiresAtMs)
    {
        public bool IsSelfSigned => SignerDeviceId == DeviceId;
    }

    public ParsedDeviceCertificate Parse(ReadOnlySpan<byte> body)
    {
        if (body.IsEmpty || body.Length > IdentityWireFormat.MaxDeviceCertificateBodyLength)
        {
            throw new DeviceCertificateFormatException("certificate body length is invalid");
        }
        var cursor = new IdentityWireReader(
            body,
            static message => new DeviceCertificateFormatException(message));
        var userId = cursor.ReadLengthPrefixedString();
        if (!Guid.TryParseExact(userId, "D", out var parsedUserId)
            || !string.Equals(
                userId,
                parsedUserId.ToString("D"),
                StringComparison.Ordinal))
        {
            throw new DeviceCertificateFormatException(
                "user_id must be a canonical lowercase UUID");
        }
        var deviceId = ReadCanonicalDeviceId(ref cursor, "device_id");
        var deviceLabel = cursor.ReadLengthPrefixedString();
        if (string.IsNullOrWhiteSpace(deviceLabel)
            || deviceLabel.Length > IdentityWireFormat.DeviceLabelMaxUtf16CodeUnits
            || !string.Equals(deviceLabel, deviceLabel.Trim(), StringComparison.Ordinal))
        {
            throw new DeviceCertificateFormatException(
                $"device_label must be canonical non-blank text of at most {IdentityWireFormat.DeviceLabelMaxUtf16CodeUnits} UTF-16 code units");
        }
        var signerDeviceId = ReadCanonicalDeviceId(ref cursor, "signer_device_id");
        var kemPub = cursor.ReadLengthPrefixedBytes();
        if (kemPub.Length != IdentityWireFormat.MlKem768PublicKeyLength)
        {
            throw new DeviceCertificateFormatException(
                $"kem_public_key must be exactly {IdentityWireFormat.MlKem768PublicKeyLength} bytes");
        }
        var sigPub = cursor.ReadLengthPrefixedBytes();
        if (sigPub.Length != IdentityWireFormat.MlDsa65PublicKeyLength)
        {
            throw new DeviceCertificateFormatException(
                $"sig_public_key must be exactly {IdentityWireFormat.MlDsa65PublicKeyLength} bytes");
        }
        var issuedAtMs = cursor.ReadUnixTimeMilliseconds("issued_at_ms");
        var expiresRaw = cursor.ReadUInt64BigEndian();
        cursor.ExpectConsumed();
        var expiresAtMs = expiresRaw == NoExpiry
            ? (long?)null
            : cursor.ToUnixTimeMilliseconds(expiresRaw, "expires_at_ms");
        if (expiresAtMs is { } expiry && expiry <= issuedAtMs)
        {
            throw new DeviceCertificateFormatException(
                "expires_at_ms must be greater than issued_at_ms");
        }

        return new ParsedDeviceCertificate(
            userId,
            deviceId,
            deviceLabel,
            signerDeviceId,
            kemPub,
            sigPub,
            issuedAtMs,
            expiresAtMs);
    }
    private static string ReadCanonicalDeviceId(
        ref IdentityWireReader cursor,
        string fieldName)
    {
        var value = cursor.ReadLengthPrefixedString();
        if (string.IsNullOrWhiteSpace(value)
            || value.Length > DeviceIdRules.MaximumLength
            || !string.Equals(value, value.Trim(), StringComparison.Ordinal))
        {
            throw new DeviceCertificateFormatException(
                $"{fieldName} must be a non-blank canonical device ID of at most {DeviceIdRules.MaximumLength} characters");
        }
        return value;
    }
}

public sealed class DeviceCertificateFormatException(string message) : Exception(message)
{
}
