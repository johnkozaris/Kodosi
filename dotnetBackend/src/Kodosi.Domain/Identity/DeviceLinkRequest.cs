
namespace Kodosi.Domain;

public sealed class DeviceLinkRequest
{
    private const int KemPublicKeyLength = IdentityWireFormat.MlKem768PublicKeyLength;
    private const int SigningPublicKeyLength = IdentityWireFormat.MlDsa65PublicKeyLength;
    private static readonly TimeSpan ApprovedResultTtl = TimeSpan.FromHours(24);

    public Guid Id { get; private set; }
    public UserId UserId { get; private set; }
    public string DeviceCode { get; private set; } = string.Empty;
    public string UserCode { get; private set; } = string.Empty;
    public string DeviceId { get; private set; } = string.Empty;
    public string DeviceLabel { get; private set; } = string.Empty;
    public byte[] KemPublicKey { get; private set; } = [];
    public byte[] SigningPublicKey { get; private set; } = [];
    public DateTimeOffset ExpiresAt { get; private set; }
    public DateTimeOffset? ApprovedAt { get; private set; }
    public DateTimeOffset? AcknowledgedAt { get; private set; }
    public DateTimeOffset? ResultExpiresAt { get; private set; }
    public DateTimeOffset? CancelledAt { get; private set; }

    public long? DeviceListGeneration { get; private set; }

    public DateTimeOffset CreatedAt { get; private set; }
    public uint Version { get; private set; }

    private DeviceLinkRequest() { }

    public static DeviceLinkRequest Create(
        UserId userId,
        string deviceCode,
        string userCode,
        string deviceId,
        string deviceLabel,
        byte[] kemPublicKey,
        byte[] signingPublicKey,
        TimeSpan ttl,
        DateTimeOffset now)
    {
        if (string.IsNullOrWhiteSpace(deviceCode))
            throw new DomainException("Device code is required.");
        if (string.IsNullOrWhiteSpace(userCode))
            throw new DomainException("User code is required.");
        var normalizedDeviceId = DeviceIdRules.Require(deviceId);
        if (kemPublicKey is not { Length: KemPublicKeyLength })
            throw new DomainException($"KEM public key must be exactly {KemPublicKeyLength} bytes.");
        if (signingPublicKey is not { Length: SigningPublicKeyLength })
            throw new DomainException($"Signing public key must be exactly {SigningPublicKeyLength} bytes.");
        if (ttl <= TimeSpan.Zero)
            throw new DomainException("Link request TTL must be positive.");

        return new DeviceLinkRequest
        {
            Id = Guid.NewGuid(),
            UserId = userId,
            DeviceCode = deviceCode,
            UserCode = userCode,
            DeviceId = normalizedDeviceId,
            DeviceLabel = TruncateLabel(deviceLabel),
            KemPublicKey = kemPublicKey,
            SigningPublicKey = signingPublicKey,
            ExpiresAt = now + ttl,
            CreatedAt = now,
        };
    }

    private static string TruncateLabel(string? label)
    {
        if (string.IsNullOrWhiteSpace(label))
            return string.Empty;
        var trimmed = label.Trim();
        return trimmed.Length > 128 ? trimmed[..128] : trimmed;
    }

    public bool MatchesInitiation(
        string deviceId,
        string? deviceLabel,
        ReadOnlySpan<byte> kemPublicKey,
        ReadOnlySpan<byte> signingPublicKey) =>
        string.Equals(DeviceId, DeviceIdRules.Normalize(deviceId), StringComparison.Ordinal)
        && string.Equals(DeviceLabel, TruncateLabel(deviceLabel), StringComparison.Ordinal)
        && KemPublicKey.AsSpan().SequenceEqual(kemPublicKey)
        && SigningPublicKey.AsSpan().SequenceEqual(signingPublicKey);

    public void Approve(long deviceListGeneration, DateTimeOffset now)
    {
        if (ApprovedAt.HasValue)
            throw new DomainException("Link request has already been approved.");
        if (CancelledAt.HasValue)
            throw new DomainException("Link request has been cancelled.");
        if (now >= ExpiresAt)
            throw new DomainException("Link request has expired.");
        if (deviceListGeneration <= 0)
            throw new DomainException("Device list generation must be positive.");

        DeviceListGeneration = deviceListGeneration;
        ApprovedAt = now;
        ResultExpiresAt = ApprovedAt.Value + ApprovedResultTtl;
    }

    public bool IsApprovedResultAvailable(DateTimeOffset now) =>
        ApprovedAt.HasValue
        && !AcknowledgedAt.HasValue
        && !CancelledAt.HasValue
        && ResultExpiresAt > now;

    public bool IsPending(DateTimeOffset now) =>
        !ApprovedAt.HasValue
        && !CancelledAt.HasValue
        && now < ExpiresAt;
}
