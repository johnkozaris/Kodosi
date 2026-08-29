namespace Kodosi.Domain;

public sealed class UserDevice
{
    private byte[] _deviceCertificate = [];
    private byte[] _deviceCertificateSignature = [];
    private DeviceCertificateParser.ParsedDeviceCertificate? _parsedCertificate;

    public UserId UserId { get; private set; }
    public string DeviceId { get; private set; } = string.Empty;
    public DateTimeOffset CreatedAt { get; private set; }
    public byte[] DeviceCertificate
    {
        get => [.. _deviceCertificate];
        private set
        {
            _deviceCertificate = [.. value];
            _parsedCertificate = null;
        }
    }
    public byte[] DeviceCertificateSignature
    {
        get => [.. _deviceCertificateSignature];
        private set => _deviceCertificateSignature = [.. value];
    }
    public DateTimeOffset? RevokedAt { get; private set; }
    public string? RevokedByDeviceId { get; private set; }

    public byte[] KemPublicKey => [.. ParsedCertificate.KemPublicKey];
    public byte[] SigningPublicKey => [.. ParsedCertificate.SigPublicKey];
    public string DeviceLabel => ParsedCertificate.DeviceLabel;
    public string CertSignerDeviceId => ParsedCertificate.SignerDeviceId;
    public DateTimeOffset CertIssuedAt => DateTimeOffset.FromUnixTimeMilliseconds(
        ParsedCertificate.IssuedAtMs);
    public DateTimeOffset? CertExpiresAt => ParsedCertificate.ExpiresAtMs is { } expiresAtMs
        ? DateTimeOffset.FromUnixTimeMilliseconds(expiresAtMs)
        : null;

    private UserDevice() { }

    public static UserDevice CreateCertified(
        UserId userId,
        string deviceId,
        byte[] certificate,
        byte[] certificateSignature,
        DateTimeOffset? validatedAt = null)
    {
        var normalizedDeviceId = DeviceIdRules.Require(deviceId);
        if (certificate is not { Length: > 0 }
            || certificate.Length > IdentityWireFormat.MaxDeviceCertificateBodyLength)
        {
            throw new DomainException("Device certificate length is invalid.");
        }
        if (certificateSignature is not { Length: IdentityWireFormat.MlDsa65SignatureLength })
            throw new DomainException("Device certificate signature length is invalid.");

        var parsed = ParseForAdmission(certificate);
        ValidateCertificateSemantics(
            parsed,
            userId,
            normalizedDeviceId,
            validatedAt ?? DateTimeOffset.UtcNow,
            requireUnexpired: true);

        return new UserDevice
        {
            UserId = userId,
            DeviceId = normalizedDeviceId,
            DeviceCertificate = [.. certificate],
            DeviceCertificateSignature = [.. certificateSignature],
            CreatedAt = DateTimeOffset.UtcNow,
        };
    }

    private DeviceCertificateParser.ParsedDeviceCertificate ParsedCertificate
    {
        get
        {
            if (_parsedCertificate is not null)
            {
                return _parsedCertificate;
            }
            try
            {
                var parsed = new DeviceCertificateParser().Parse(_deviceCertificate);
                try
                {
                    ValidateCertificateSemantics(
                        parsed,
                        UserId,
                        DeviceId,
                        validatedAt: default,
                        requireUnexpired: false);
                }
                catch (DomainException exception)
                {
                    throw Corrupt(exception.Message, exception);
                }
                _parsedCertificate = parsed;
                return parsed;
            }
            catch (DeviceCertificateFormatException exception)
            {
                throw Corrupt(exception.Message, exception);
            }
        }
    }

    public void RequireCertificateIntegrity()
    {
        if (_deviceCertificate is not { Length: > 0 }
            || _deviceCertificate.Length > IdentityWireFormat.MaxDeviceCertificateBodyLength)
        {
            throw Corrupt("certificate length is invalid");
        }
        if (_deviceCertificateSignature.Length != IdentityWireFormat.MlDsa65SignatureLength)
        {
            throw Corrupt("certificate signature length is invalid");
        }
        _ = ParsedCertificate;
    }

    public bool MatchesEnrollment(
        UserId userId,
        string deviceId,
        ReadOnlySpan<byte> kemPublicKey,
        ReadOnlySpan<byte> signingPublicKey,
        ReadOnlySpan<byte> certificate,
        ReadOnlySpan<byte> certificateSignature) =>
        UserId == userId
        && string.Equals(DeviceId, DeviceIdRules.Normalize(deviceId), StringComparison.Ordinal)
        && ParsedCertificate.KemPublicKey.AsSpan().SequenceEqual(kemPublicKey)
        && ParsedCertificate.SigPublicKey.AsSpan().SequenceEqual(signingPublicKey)
        && _deviceCertificate.AsSpan().SequenceEqual(certificate)
        && _deviceCertificateSignature.AsSpan().SequenceEqual(certificateSignature);

    public void Revoke(string revokedByDeviceId)
    {
        var normalizedRevokerDeviceId = DeviceIdRules.Require(
            revokedByDeviceId,
            "Revoked-by device ID");
        if (RevokedAt.HasValue)
            return;
        RevokedAt = DateTimeOffset.UtcNow;
        RevokedByDeviceId = normalizedRevokerDeviceId;
    }

    private static DeviceCertificateParser.ParsedDeviceCertificate ParseForAdmission(
        ReadOnlySpan<byte> certificate)
    {
        try
        {
            return new DeviceCertificateParser().Parse(certificate);
        }
        catch (DeviceCertificateFormatException exception)
        {
            throw new DomainException(
                $"Invalid device certificate: {exception.Message}",
                innerException: exception);
        }
    }

    private static void ValidateCertificateSemantics(
        DeviceCertificateParser.ParsedDeviceCertificate parsed,
        UserId userId,
        string deviceId,
        DateTimeOffset validatedAt,
        bool requireUnexpired)
    {
        if (parsed.DeviceId != deviceId)
            throw new DomainException("Certificate device ID does not match the persistence key.");
        if (parsed.UserId != userId.Value.ToString("D"))
            throw new DomainException("Certificate user ID does not match the persistence owner.");
        if (requireUnexpired
            && parsed.ExpiresAtMs is { } activeExpiry
            && activeExpiry <= validatedAt.ToUnixTimeMilliseconds())
        {
            throw new DomainException("Cert expiry must not be in the past.");
        }
    }

    private DeviceCertificateCorruptionException Corrupt(
        string reason,
        Exception? innerException = null) =>
        new(UserId, DeviceId, reason, innerException);
}

public sealed class DeviceCertificateCorruptionException(
    UserId userId,
    string deviceId,
    string reason,
    Exception? innerException = null)
    : DomainException(
        $"Stored device certificate for user {userId.Value} and device {deviceId} is corrupt: {reason}",
        "DEVICE_CERTIFICATE_CORRUPT",
        innerException);
