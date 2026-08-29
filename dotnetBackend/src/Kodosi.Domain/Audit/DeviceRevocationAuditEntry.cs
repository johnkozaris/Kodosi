using System.Text.Json;

namespace Kodosi.Domain;

public readonly record struct DeviceRevocationSessionTarget(
    SessionId SessionId,
    Guid IncarnationId);

public sealed class DeviceRevocationAuditEntry
{
    public Guid Id { get; private set; }
    public Guid RevocationId { get; private set; }
    public UserId ActorUserId { get; private set; }
    public string RevokedDeviceId { get; private set; } = string.Empty;
    public string SignerDeviceId { get; private set; } = string.Empty;
    public long NewGeneration { get; private set; }
    public long IdentityRevision { get; private set; }
    public int BlobsCascaded { get; private set; }
    public Guid[] AffectedSessionIds { get; private set; } = [];
    public string? AffectedSessionTargetsJson { get; private set; }
    public DateTimeOffset OccurredAt { get; private set; }
    public DateTimeOffset? RealtimeEnforcedAt { get; private set; }
    public string? ClientIp { get; private set; }
    public string? UserAgent { get; private set; }

    private DeviceRevocationAuditEntry() { }

    public static DeviceRevocationAuditEntry Create(
        Guid revocationId,
        UserId actorUserId,
        string revokedDeviceId,
        string signerDeviceId,
        long newGeneration,
        long identityRevision,
        int blobsCascaded,
        IReadOnlyCollection<DeviceRevocationSessionTarget> affectedSessionTargets,
        string? clientIp,
        string? userAgent)
    {
        if (revocationId == Guid.Empty)
            throw new DomainException("RevocationId is required.");
        if (string.IsNullOrWhiteSpace(revokedDeviceId))
            throw new DomainException("RevokedDeviceId is required.");
        if (string.IsNullOrWhiteSpace(signerDeviceId))
            throw new DomainException("SignerDeviceId is required.");
        if (string.Equals(revokedDeviceId, signerDeviceId, StringComparison.Ordinal))
            throw new DomainException(
                "SignerDeviceId and RevokedDeviceId must differ (backend refuses self-signed suicide).");
        if (newGeneration < 1)
            throw new DomainException("NewGeneration must be positive.");
        if (identityRevision < 1)
            throw new DomainException("IdentityRevision must be positive.");
        if (blobsCascaded < 0)
            throw new DomainException("BlobsCascaded must be non-negative.");

        var normalizedTargets = affectedSessionTargets
            .OrderBy(target => target.SessionId.Value)
            .ThenBy(target => target.IncarnationId)
            .ToArray();
        if (normalizedTargets.Any(target =>
                target.SessionId.Value == Guid.Empty
                || target.IncarnationId == Guid.Empty)
            || normalizedTargets
                .GroupBy(target => target.SessionId)
                .Any(group => group.Count() != 1))
        {
            throw new DomainException(
                "Affected session targets require non-empty session and incarnation IDs.");
        }

        return new DeviceRevocationAuditEntry
        {
            Id = Guid.NewGuid(),
            RevocationId = revocationId,
            ActorUserId = actorUserId,
            RevokedDeviceId = revokedDeviceId,
            SignerDeviceId = signerDeviceId,
            NewGeneration = newGeneration,
            IdentityRevision = identityRevision,
            BlobsCascaded = blobsCascaded,


            AffectedSessionIds = normalizedTargets
                .Select(target => target.SessionId.Value)
                .Distinct()
                .Order()
                .ToArray(),
            AffectedSessionTargetsJson = JsonSerializer.Serialize(normalizedTargets),
            OccurredAt = DateTimeOffset.UtcNow,
            ClientIp = clientIp,
            UserAgent = userAgent,
        };
    }

    public IReadOnlyList<DeviceRevocationSessionTarget> GetAffectedSessionTargets()
    {
        if (AffectedSessionTargetsJson is null)
        {


            return [];
        }

        try
        {
            var targets = JsonSerializer.Deserialize<DeviceRevocationSessionTarget[]>(
                AffectedSessionTargetsJson);
            if (targets is null
                || targets.Any(target =>
                    target.SessionId.Value == Guid.Empty
                    || target.IncarnationId == Guid.Empty)
                || targets
                    .GroupBy(target => target.SessionId)
                    .Any(group => group.Count() != 1))
            {
                throw InvalidTargetEvidence();
            }

            var targetSessionIds = targets
                .Select(target => target.SessionId.Value)
                .Distinct()
                .Order()
                .ToArray();
            var auditedSessionIds = AffectedSessionIds
                .Distinct()
                .Order()
                .ToArray();
            if (!targetSessionIds.SequenceEqual(auditedSessionIds))
            {
                throw InvalidTargetEvidence();
            }

            return targets
                .OrderBy(target => target.SessionId.Value)
                .ThenBy(target => target.IncarnationId)
                .ToArray();
        }
        catch (JsonException)
        {
            throw InvalidTargetEvidence();
        }
    }

    private static DomainException InvalidTargetEvidence() =>
        new("Affected session target evidence is invalid.");
}
