using System.Text.Json;

namespace Kodosi.Domain;

public readonly record struct IdentityResetSessionTarget(
    SessionId SessionId,
    Guid IncarnationId);

public readonly record struct IdentityResetEndedSessionTarget(
    SessionId SessionId,
    Guid IncarnationId,
    UserId OwnerUserId,
    SessionScope Scope,
    RoomId? RoomId,
    DateTimeOffset StartedAt);

public sealed class IdentityResetAuditEntry
{
    public Guid Id { get; private set; }
    public UserId UserId { get; private set; }
    public DateTimeOffset ResetAt { get; private set; }
    public int SessionsAttempted { get; private set; }
    public int SessionsEnded { get; private set; }
    public int DevicesRemoved { get; private set; }
    public int DeviceListsRemoved { get; private set; }
    public string? ClientIp { get; private set; }
    public string? UserAgent { get; private set; }
    public string[] RemovedDeviceIds { get; private set; } = [];
    public Guid[] EndedSessionIds { get; private set; } = [];
    public Guid[] SessionsWithRevokedKeys { get; private set; } = [];
    public Guid[] AudienceUserIds { get; private set; } = [];
    public string? SessionTargetsJson { get; private set; }
    public string? EndedSessionTargetsJson { get; private set; }
    public long IdentityRevision { get; private set; }
    public DateTimeOffset? RealtimeEnforcedAt { get; private set; }

    private IdentityResetAuditEntry() { }

    public static IdentityResetAuditEntry Create(
        Guid id,
        UserId userId,
        int sessionsAttempted,
        int sessionsEnded,
        int devicesRemoved,
        int deviceListsRemoved,
        string? clientIp,
        string? userAgent,
        IReadOnlyCollection<string> removedDeviceIds,
        IReadOnlyCollection<SessionId> endedSessionIds,
        IReadOnlyCollection<IdentityResetEndedSessionTarget> endedSessionTargets,
        IReadOnlyCollection<SessionId> sessionsWithRevokedKeys,
        IReadOnlyCollection<IdentityResetSessionTarget> sessionTargets,
        IReadOnlyCollection<UserId> audienceUserIds,
        long identityRevision)
    {
        if (id == Guid.Empty)
        {
            throw new DomainException("Identity reset ID cannot be empty.");
        }

        if (identityRevision <= 0)
        {
            throw new DomainException("Identity reset revision must be positive.");
        }

        var normalizedSessionTargets = NormalizeSessionTargets(sessionTargets);
        var normalizedEndedTargets = NormalizeEndedTargets(endedSessionTargets);
        var exactSessionIds = normalizedSessionTargets
            .Select(target => target.SessionId)
            .ToHashSet();
        if (endedSessionIds
                .Concat(sessionsWithRevokedKeys)
                .Distinct()
                .Any(sessionId => !exactSessionIds.Contains(sessionId)))
        {
            throw new DomainException(
                "Every affected identity reset session requires an exact incarnation target.");
        }
        if (normalizedEndedTargets.Any(ended =>
                !normalizedSessionTargets.Contains(
                    new IdentityResetSessionTarget(ended.SessionId, ended.IncarnationId))))
        {
            throw new DomainException(
                "Ended session targets must be included in the identity reset target set.");
        }

        return new IdentityResetAuditEntry
        {
            Id = id,
            UserId = userId,
            ResetAt = DateTimeOffset.UtcNow,
            SessionsAttempted = sessionsAttempted,
            SessionsEnded = sessionsEnded,
            DevicesRemoved = devicesRemoved,
            DeviceListsRemoved = deviceListsRemoved,
            ClientIp = clientIp,
            UserAgent = userAgent,
            RemovedDeviceIds = removedDeviceIds
                .Distinct(StringComparer.Ordinal)
                .Order(StringComparer.Ordinal)
                .ToArray(),
            EndedSessionIds = endedSessionIds
                .Select(sessionId => sessionId.Value)
                .Distinct()
                .Order()
                .ToArray(),
            SessionsWithRevokedKeys = sessionsWithRevokedKeys
                .Select(sessionId => sessionId.Value)
                .Distinct()
                .Order()
                .ToArray(),
            AudienceUserIds = audienceUserIds
                .Select(audienceUserId => audienceUserId.Value)
                .Where(audienceUserId => audienceUserId != userId.Value)
                .Distinct()
                .Order()
                .ToArray(),
            SessionTargetsJson = JsonSerializer.Serialize(normalizedSessionTargets),
            EndedSessionTargetsJson = JsonSerializer.Serialize(normalizedEndedTargets),
            IdentityRevision = identityRevision,
        };
    }

    public IReadOnlyList<IdentityResetSessionTarget> GetSessionTargets()
    {
        if (SessionTargetsJson is null)
        {


            return [];
        }

        try
        {
            var targets = JsonSerializer.Deserialize<IdentityResetSessionTarget[]>(
                SessionTargetsJson);
            if (targets is null)
            {
                throw InvalidTargetEvidence();
            }
            var normalized = NormalizeSessionTargets(targets);
            var auditedIds = EndedSessionIds
                .Concat(SessionsWithRevokedKeys)
                .Distinct()
                .Order()
                .ToArray();
            var targetIds = normalized
                .Select(target => target.SessionId.Value)
                .Distinct()
                .Order()
                .ToArray();
            if (auditedIds.Except(targetIds).Any())
            {
                throw InvalidTargetEvidence();
            }
            return normalized;
        }
        catch (JsonException)
        {
            throw InvalidTargetEvidence();
        }
    }

    public IReadOnlyList<IdentityResetEndedSessionTarget> GetEndedSessionTargets()
    {
        if (EndedSessionTargetsJson is null)
        {
            return [];
        }

        try
        {
            var targets = JsonSerializer.Deserialize<IdentityResetEndedSessionTarget[]>(
                EndedSessionTargetsJson);
            if (targets is null)
            {
                throw InvalidTargetEvidence();
            }
            var normalized = NormalizeEndedTargets(targets);
            var auditedEndedIds = EndedSessionIds.Distinct().ToHashSet();
            if (normalized.Any(target => !auditedEndedIds.Contains(target.SessionId.Value)))
            {
                throw InvalidTargetEvidence();
            }
            var exactTargets = GetSessionTargets().ToHashSet();
            if (normalized.Any(target => !exactTargets.Contains(
                    new IdentityResetSessionTarget(
                        target.SessionId,
                        target.IncarnationId))))
            {
                throw InvalidTargetEvidence();
            }
            return normalized;
        }
        catch (JsonException)
        {
            throw InvalidTargetEvidence();
        }
    }

    private static IdentityResetSessionTarget[] NormalizeSessionTargets(
        IEnumerable<IdentityResetSessionTarget> targets)
    {
        var normalized = targets
            .OrderBy(target => target.SessionId.Value)
            .ThenBy(target => target.IncarnationId)
            .ToArray();
        if (normalized.Any(target =>
                target.SessionId.Value == Guid.Empty
                || target.IncarnationId == Guid.Empty)
            || normalized
                .GroupBy(target => target.SessionId)
                .Any(group => group.Count() != 1))
        {
            throw InvalidTargetEvidence();
        }
        return normalized;
    }

    private static IdentityResetEndedSessionTarget[] NormalizeEndedTargets(
        IEnumerable<IdentityResetEndedSessionTarget> targets)
    {
        var normalized = targets
            .OrderBy(target => target.SessionId.Value)
            .ThenBy(target => target.IncarnationId)
            .ToArray();
        if (normalized.Any(target =>
                target.SessionId.Value == Guid.Empty
                || target.IncarnationId == Guid.Empty
                || target.OwnerUserId.Value == Guid.Empty
                || target.StartedAt == default
                || target.RoomId is { Value: var roomId } && roomId == Guid.Empty)
            || normalized
                .GroupBy(target => target.SessionId)
                .Any(group => group.Count() != 1))
        {
            throw InvalidTargetEvidence();
        }
        return normalized;
    }

    private static DomainException InvalidTargetEvidence() =>
        new("Identity reset session target evidence is invalid.");
}
