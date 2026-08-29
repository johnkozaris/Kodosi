using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class AccessOverrideAuditEntryTests
{
    [Fact]
    public void Expired_Revocation_Requires_Dedicated_Exact_Evidence_Factory()
    {
        var exception = Assert.Throws<DomainException>(() =>
            AccessOverrideAuditEntry.Create(
                SessionId.New(),
                UserId.New(),
                UserId.New(),
                AccessOverrideAuditAction.Revoked,
                AccessOverrideAuditReason.Expired,
                null,
                null));

        Assert.Equal(
            "Expired revocations require exact realtime enforcement evidence.",
            exception.Message);
    }

    [Fact]
    public void Expired_Revocation_Captures_Exact_Pending_Work_Tuple()
    {
        var sessionId = SessionId.New();
        var actorUserId = UserId.New();
        var granteeUserId = UserId.New();
        var incarnationId = Guid.CreateVersion7();
        var startedAt = new DateTimeOffset(2026, 8, 20, 10, 0, 0, TimeSpan.Zero);
        var expiresAt = startedAt.AddMinutes(5);
        var revokedAt = expiresAt.AddSeconds(1);

        var entry = AccessOverrideAuditEntry.CreateExpiredRevocation(
            sessionId,
            actorUserId,
            granteeUserId,
            incarnationId,
            startedAt,
            expiresAt,
            revokedAt);

        Assert.Equal(sessionId, entry.SessionId);
        Assert.Equal(actorUserId, entry.ActorUserId);
        Assert.Equal(granteeUserId, entry.GranteeUserId);
        Assert.Equal(AccessOverrideAuditAction.Revoked, entry.Action);
        Assert.Equal(AccessOverrideAuditReason.Expired, entry.Reason);
        Assert.Equal(revokedAt, entry.OccurredAt);
        Assert.Equal(incarnationId, entry.SessionIncarnationId);
        Assert.Equal(startedAt, entry.SessionStartedAt);
        Assert.Equal(expiresAt, entry.ExpectedExpiresAt);
        Assert.Equal(revokedAt, entry.ExpectedRevokedAt);
        Assert.Null(entry.RealtimeEnforcedAt);
    }

    [Fact]
    public void Expired_Revocation_Rejects_Impossible_Timestamps()
    {
        var revokedAt = new DateTimeOffset(2026, 8, 20, 10, 0, 0, TimeSpan.Zero);

        Assert.Throws<DomainException>(() =>
            AccessOverrideAuditEntry.CreateExpiredRevocation(
                SessionId.New(),
                UserId.New(),
                UserId.New(),
                Guid.CreateVersion7(),
                revokedAt,
                revokedAt.AddTicks(1),
                revokedAt));
    }
}
