namespace Kodosi.Domain;

public sealed class AccessOverrideAuditEntry
{
    public Guid Id { get; private set; }
    public SessionId SessionId { get; private set; }
    public UserId ActorUserId { get; private set; }
    public UserId GranteeUserId { get; private set; }
    public AccessOverrideAuditAction Action { get; private set; }
    public AccessOverrideAuditReason Reason { get; private set; }
    public DateTimeOffset OccurredAt { get; private set; }
    public Guid? SessionIncarnationId { get; private set; }
    public DateTimeOffset? SessionStartedAt { get; private set; }
    public DateTimeOffset? ExpectedExpiresAt { get; private set; }
    public DateTimeOffset? ExpectedRevokedAt { get; private set; }
    public DateTimeOffset? RealtimeEnforcedAt { get; private set; }
    public string? ClientIp { get; private set; }
    public string? UserAgent { get; private set; }

    private AccessOverrideAuditEntry() { }

    public static AccessOverrideAuditEntry Create(
        SessionId sessionId,
        UserId actorUserId,
        UserId granteeUserId,
        AccessOverrideAuditAction action,
        AccessOverrideAuditReason reason,
        string? clientIp,
        string? userAgent)
    {
        if (reason == AccessOverrideAuditReason.Expired)
        {
            throw new DomainException(
                "Expired revocations require exact realtime enforcement evidence.");
        }

        var occurredAt = DateTimeOffset.UtcNow;
        return new AccessOverrideAuditEntry
        {
            Id = Guid.NewGuid(),
            SessionId = sessionId,
            ActorUserId = actorUserId,
            GranteeUserId = granteeUserId,
            Action = action,
            Reason = reason,
            OccurredAt = occurredAt,
            RealtimeEnforcedAt = occurredAt,
            ClientIp = clientIp,
            UserAgent = userAgent,
        };
    }

    public static AccessOverrideAuditEntry CreateExpiredRevocation(
        SessionId sessionId,
        UserId actorUserId,
        UserId granteeUserId,
        Guid sessionIncarnationId,
        DateTimeOffset sessionStartedAt,
        DateTimeOffset expectedExpiresAt,
        DateTimeOffset expectedRevokedAt)
    {
        if (sessionId.Value == Guid.Empty)
            throw new DomainException("SessionId is required.");
        if (actorUserId.Value == Guid.Empty)
            throw new DomainException("ActorUserId is required.");
        if (granteeUserId.Value == Guid.Empty)
            throw new DomainException("GranteeUserId is required.");
        if (sessionIncarnationId == Guid.Empty)
            throw new DomainException("SessionIncarnationId is required.");
        if (sessionStartedAt == default)
            throw new DomainException("SessionStartedAt is required.");
        if (expectedExpiresAt == default)
            throw new DomainException("ExpectedExpiresAt is required.");
        if (expectedRevokedAt == default)
            throw new DomainException("ExpectedRevokedAt is required.");
        if (expectedExpiresAt > expectedRevokedAt)
            throw new DomainException(
                "ExpectedExpiresAt must not be later than ExpectedRevokedAt.");

        return new AccessOverrideAuditEntry
        {
            Id = Guid.NewGuid(),
            SessionId = sessionId,
            ActorUserId = actorUserId,
            GranteeUserId = granteeUserId,
            Action = AccessOverrideAuditAction.Revoked,
            Reason = AccessOverrideAuditReason.Expired,
            OccurredAt = expectedRevokedAt,
            SessionIncarnationId = sessionIncarnationId,
            SessionStartedAt = sessionStartedAt,
            ExpectedExpiresAt = expectedExpiresAt,
            ExpectedRevokedAt = expectedRevokedAt,
        };
    }
}

public enum AccessOverrideAuditAction
{
    Granted,
    Revoked,
}

public enum AccessOverrideAuditReason
{
    Explicit,
    ScopeNarrowing,
    Expired,
}
