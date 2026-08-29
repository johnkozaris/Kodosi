using System.Text.Json;
using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class IdentityResetAuditEntryTests
{
    [Fact]
    public void Exact_Targets_Round_Trip_With_Ended_Projection_Evidence()
    {
        var userId = UserId.New();
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var startedAt = DateTimeOffset.UtcNow;
        var entry = IdentityResetAuditEntry.Create(
            Guid.CreateVersion7(),
            userId,
            sessionsAttempted: 1,
            sessionsEnded: 1,
            devicesRemoved: 1,
            deviceListsRemoved: 1,
            clientIp: null,
            userAgent: null,
            removedDeviceIds: ["device"],
            endedSessionIds: [sessionId],
            endedSessionTargets:
            [
                new IdentityResetEndedSessionTarget(
                    sessionId,
                    incarnationId,
                    userId,
                    SessionScope.Friends,
                    null,
                    startedAt),
            ],
            sessionsWithRevokedKeys: [sessionId],
            sessionTargets:
            [
                new IdentityResetSessionTarget(sessionId, incarnationId),
            ],
            audienceUserIds: [],
            identityRevision: 1);

        Assert.Equal(
            [new IdentityResetSessionTarget(sessionId, incarnationId)],
            entry.GetSessionTargets());
        var ended = Assert.Single(entry.GetEndedSessionTargets());
        Assert.Equal(sessionId, ended.SessionId);
        Assert.Equal(incarnationId, ended.IncarnationId);
        Assert.Equal(startedAt, ended.StartedAt);
    }

    [Fact]
    public void Legacy_Bare_Ids_Are_Audit_Only()
    {
        var sessionId = SessionId.New();
        var entry = CreateEmptyEntry();
        DomainFixtureHydrator.SetIdentityResetTargetEvidence(
            entry,
            sessionTargetsJson: null,
            endedSessionTargetsJson: null,
            endedSessionIds: [sessionId.Value],
            sessionsWithRevokedKeys: [sessionId.Value]);

        Assert.Empty(entry.GetSessionTargets());
        Assert.Empty(entry.GetEndedSessionTargets());
    }

    [Fact]
    public void Malformed_Or_Rebound_Target_Evidence_Fails_Closed()
    {
        var sessionId = SessionId.New();
        var entry = CreateEmptyEntry();
        DomainFixtureHydrator.SetIdentityResetTargetEvidence(
            entry,
            sessionTargetsJson: "null",
            endedSessionTargetsJson: "[]",
            endedSessionIds: [],
            sessionsWithRevokedKeys: [sessionId.Value]);
        Assert.Throws<DomainException>(() => entry.GetSessionTargets());

        DomainFixtureHydrator.SetIdentityResetTargetEvidence(
            entry,
            sessionTargetsJson: JsonSerializer.Serialize(new[]
            {
                new IdentityResetSessionTarget(sessionId, Guid.CreateVersion7()),
            }),
            endedSessionTargetsJson: "[]",
            endedSessionIds: [],
            sessionsWithRevokedKeys: [SessionId.New().Value]);
        Assert.Throws<DomainException>(() => entry.GetSessionTargets());
    }

    [Fact]
    public void Duplicate_Session_Targets_Are_Rejected()
    {
        var sessionId = SessionId.New();
        var userId = UserId.New();
        Assert.Throws<DomainException>(() => IdentityResetAuditEntry.Create(
            Guid.CreateVersion7(),
            userId,
            0,
            0,
            0,
            0,
            null,
            null,
            [],
            [],
            [],
            [],
            [
                new IdentityResetSessionTarget(sessionId, Guid.CreateVersion7()),
                new IdentityResetSessionTarget(sessionId, Guid.CreateVersion7()),
            ],
            [],
            1));
    }

    private static IdentityResetAuditEntry CreateEmptyEntry() =>
        IdentityResetAuditEntry.Create(
            Guid.CreateVersion7(),
            UserId.New(),
            0,
            0,
            0,
            0,
            null,
            null,
            [],
            [],
            [],
            [],
            [],
            [],
            1);
}
