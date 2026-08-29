using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;
using Microsoft.Extensions.Logging.Abstractions;

namespace Kodosi.HostTests;

public sealed class IdentityLifecycleServiceTests
{
    private const string GoodSignatureMarker = "valid";
    private const string BadSignatureMarker = "invalid";
    private static readonly byte[] DummyKemKey = new byte[1184];
    private static readonly byte[] DummySignKey = new byte[1952];

    private static readonly IdentityResetAuditContext AuditCtx = new(
        ClientIp: "127.0.0.1",
        UserAgent: "xunit");

    [Fact]
    public async Task Pending_Reset_Enforcement_Replays_Under_Fences_And_Completes_Marker()
    {
        var userId = UserId.New();
        var resetId = Guid.NewGuid();
        var endedSessionId = SessionId.New();
        var endedIncarnationId = Guid.CreateVersion7();
        var revokedKeySessionId = SessionId.New();
        var revokedKeyIncarnationId = Guid.CreateVersion7();
        var authority = new FakeSessionEndAuthority();
        var enforcer = new RecordingIdentityResetRealtimeEnforcer(authority);
        var durability = new FakeIdentityResetDurabilityCoordinator(
            [
                new IdentityResetEnforcementWork(
                    resetId,
                    userId,
                    1,
                    ["removed-device"],
                    [endedSessionId],
                    [revokedKeySessionId])
                {
                    SessionTargets =
                    [
                        new IdentityResetSessionTarget(
                            endedSessionId,
                            endedIncarnationId),
                        new IdentityResetSessionTarget(
                            revokedKeySessionId,
                            revokedKeyIncarnationId),
                    ],
                    EndedSessions =
                    [
                        new IdentityResetEndedSession(
                            new SessionDiscoveryTarget(
                                endedSessionId,
                                userId,
                                SessionScope.JustMe,
                                null,
                                endedIncarnationId),
                            DateTimeOffset.UtcNow),
                    ],
                }
            ]);
        var worker = new IdentityResetEnforcementHostedService(
            durability,
            enforcer,
            new PassThroughIdentityResetSessionResolver(),
            authority,
            NullLogger<IdentityResetEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.Equal(
            new[] { endedSessionId, revokedKeySessionId }
                .OrderBy(sessionId => sessionId.Value),
            authority.AcquiredSessionIds);
        Assert.True(enforcer.EnforcedBeforeLeaseRelease);
        Assert.True(enforcer.FenceHeldDuringEnforcement);
        Assert.Single(authority.Retired);
        Assert.Equal(endedSessionId, authority.Retired[0].Target.SessionId);
        Assert.True(authority.ProjectedBeforeLeaseRelease);
        Assert.Equal([resetId], durability.CompletedResetIds);
    }

    [Fact]
    public async Task Pending_Reset_Enforcement_Does_Not_Target_A_Republished_Incarnation()
    {
        var userId = UserId.New();
        var resetId = Guid.CreateVersion7();
        var sessionId = SessionId.New();
        var retiredIncarnation = Guid.CreateVersion7();
        var work = new IdentityResetEnforcementWork(
            resetId,
            userId,
            1,
            ["removed-device"],
            [sessionId],
            [sessionId])
        {
            SessionTargets =
            [
                new IdentityResetSessionTarget(sessionId, retiredIncarnation),
            ],
            EndedSessions =
            [
                new IdentityResetEndedSession(
                    new SessionDiscoveryTarget(
                        sessionId,
                        userId,
                        SessionScope.Friends,
                        null,
                        retiredIncarnation),
                    DateTimeOffset.UtcNow),
            ],
        };
        var durability = new FakeIdentityResetDurabilityCoordinator([work]);
        var authority = new FakeSessionEndAuthority();
        var enforcer = new RecordingIdentityResetRealtimeEnforcer(authority);
        var resolverCalls = 0;
        var resolver = new PassThroughIdentityResetSessionResolver
        {
            Resolve = targets =>
            {
                resolverCalls++;
                return resolverCalls == 1
                    ? targets.ToList()
                    : [];
            },
        };
        var worker = new IdentityResetEnforcementHostedService(
            durability,
            enforcer,
            resolver,
            authority,
            NullLogger<IdentityResetEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.Equal([sessionId], authority.AcquiredSessionIds);
        Assert.Empty(authority.Retired);
        Assert.Empty(enforcer.SessionsWithRevokedKeys);
        Assert.Equal([resetId], durability.CompletedResetIds);
    }

    [Fact]
    public async Task Timed_Out_Reset_Enforcement_Remains_Pending_For_Retry()
    {
        var work = new IdentityResetEnforcementWork(
            Guid.NewGuid(),
            UserId.New(),
            1,
            ["removed-device"],
            [],
            []);
        var durability = new FakeIdentityResetDurabilityCoordinator([work])
        {
            BlockUntilCancelled = true,
        };
        var authority = new FakeSessionEndAuthority();
        var worker = new IdentityResetEnforcementHostedService(
            durability,
            new RecordingIdentityResetRealtimeEnforcer(authority),
            new PassThroughIdentityResetSessionResolver(),
            authority,
            NullLogger<IdentityResetEnforcementHostedService>.Instance,
            TimeProvider.System)
        {
            EnforcementAttemptTimeout = TimeSpan.FromMilliseconds(20),
        };

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.True(durability.AttemptWasCancelled);
        Assert.Empty(durability.CompletedResetIds);
    }

    [Fact]
    public async Task Obsolete_Reset_Enforcement_Completes_Without_Touching_Current_Epoch()
    {
        var userId = UserId.New();
        var resetId = Guid.NewGuid();
        var authority = new FakeSessionEndAuthority();
        var enforcer = new RecordingIdentityResetRealtimeEnforcer(authority);
        var durability = new FakeIdentityResetDurabilityCoordinator(
            [
                new IdentityResetEnforcementWork(
                    resetId,
                    userId,
                    1,
                    ["reused-device"],
                    [],
                    []),
            ])
        {
            IsCurrent = false,
        };
        var worker = new IdentityResetEnforcementHostedService(
            durability,
            enforcer,
            new PassThroughIdentityResetSessionResolver(),
            authority,
            NullLogger<IdentityResetEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.Equal([resetId], durability.CompletedResetIds);
        Assert.False(authority.LeaseHeld);
        Assert.False(enforcer.FenceHeldDuringEnforcement);
        Assert.False(enforcer.EnforcedBeforeLeaseRelease);
    }

    [Fact]
    public async Task ResetIdentity_BurnsChallenge_BeforeRunningSignatureVerification()
    {
        var userId = UserId.New();
        var device = CreateEnrolledDevice(userId, out var deviceId);
        var challenge = CreateChallenge(userId);
        var fixture = BuildFixture(
            userId: userId,
            devices: [device],
            challenges: [challenge]);

        var badPayload = new IdentityResetPopPayload(
            ChallengeId: challenge.Id,
            SignerDeviceId: deviceId,
            PopSignature: EncodeSignature(BadSignatureMarker));

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.ResetIdentityAsync(
                userId, badPayload, AuditCtx, TestContext.Current.CancellationToken));

        Assert.True(
            fixture.Verifier.ChallengeConsumedAtVerifyTime,
            "challenge must already be consumed when Verify() is invoked");
        Assert.Contains(
            (PopFailureReason.SignatureInvalid, "identity_reset"),
            fixture.Metrics.PopFailures);

        fixture.Metrics.PopFailures.Clear();
        var goodReplay = new IdentityResetPopPayload(
            ChallengeId: challenge.Id,
            SignerDeviceId: deviceId,
            PopSignature: EncodeSignature(GoodSignatureMarker));

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.ResetIdentityAsync(
                userId, goodReplay, AuditCtx, TestContext.Current.CancellationToken));

        Assert.Contains(
            PopFailureReason.ChallengeExpired,
            fixture.Metrics.PopFailures.Select(f => f.Reason));
        Assert.Empty(fixture.Audit.Entries);
        Assert.NotEmpty(
            await fixture.Devices.GetByUserIdAsync(userId, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ResetIdentity_Holds_Session_Authority_Through_Committed_Projection()
    {
        var userId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            userId,
            "identity-reset-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var fixture = BuildFixture(
            userId,
            devices: [],
            challenges: [],
            sessions: [session]);

        var result = await fixture.Service.ResetIdentityAsync(
            userId,
            popPayload: null,
            AuditCtx,
            TestContext.Current.CancellationToken);

        Assert.Equal([session.Id], fixture.SessionEndAuthority.AcquiredSessionIds);
        var projected = Assert.Single(fixture.SessionEndAuthority.Projected);
        Assert.Equal(session.Id, projected.Transition.SharingState?.SessionId);
        Assert.Equal(SessionTransitionOutcome.Applied, projected.Transition.Outcome);
        Assert.Equal(
            CommittedSessionEndReason.OwnerIdentityReset,
            projected.Reason);
        Assert.True(fixture.SessionEndAuthority.ProjectedBeforeLeaseRelease);
        Assert.True(
            Assert.IsType<RecordingIdentityResetRealtimeEnforcer>(
                fixture.RealtimeEnforcer)
            .EnforcedBeforeLeaseRelease);
        Assert.True(
            Assert.IsType<RecordingIdentityResetRealtimeEnforcer>(
                fixture.RealtimeEnforcer)
            .FenceHeldDuringEnforcement);
        Assert.Equal([session.Id], result.EndedSessionIds);
        Assert.Equal(1, result.IdentityRevision);
        Assert.Equal([userId], fixture.DeviceLinks.InvalidatedUsers);
    }

    [Fact]
    public async Task ResetIdentity_Reports_Attempted_And_Ended_Sessions_Separately()
    {
        var userId = UserId.New();
        var session = Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            1,
            Session.CurrentIncarnationProtocolVersion,
            userId,
            "identity-reset-missing-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        var fixture = BuildFixture(
            userId,
            devices: [],
            challenges: [],
            sessions: [session],
            dropSessionAfterTargetResolution: session.Id);

        var result = await fixture.Service.ResetIdentityAsync(
            userId,
            popPayload: null,
            AuditCtx,
            TestContext.Current.CancellationToken);

        Assert.Equal(1, result.SessionsAttempted);
        Assert.Empty(result.EndedSessionIds);
        var audit = Assert.Single(fixture.Audit.Entries);
        Assert.Equal(1, audit.SessionsAttempted);
        Assert.Equal(0, audit.SessionsEnded);
    }

    [Fact]
    public async Task ResetIdentity_Fails_When_TryConsume_Loses_Concurrency_Race()
    {
        var userId = UserId.New();
        var device = CreateEnrolledDevice(userId, out var deviceId);
        var challenge = CreateChallenge(userId);
        var fixture = BuildFixture(
            userId: userId,
            devices: [device],
            challenges: [challenge],
            losePersistRace: true);

        var payload = new IdentityResetPopPayload(
            ChallengeId: challenge.Id,
            SignerDeviceId: deviceId,
            PopSignature: EncodeSignature(GoodSignatureMarker));

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.ResetIdentityAsync(
                userId, payload, AuditCtx, TestContext.Current.CancellationToken));

        Assert.Contains(
            PopFailureReason.ChallengeExpired,
            fixture.Metrics.PopFailures.Select(f => f.Reason));
        Assert.Empty(fixture.Audit.Entries);
        Assert.NotEmpty(
            await fixture.Devices.GetByUserIdAsync(userId, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ResetIdentity_Fails_When_Challenge_Missing_With_ChallengeExpired_Metric()
    {
        var userId = UserId.New();
        var device = CreateEnrolledDevice(userId, out var deviceId);
        var fixture = BuildFixture(
            userId: userId,
            devices: [device],
            challenges: []);

        var payload = new IdentityResetPopPayload(
            ChallengeId: Guid.NewGuid(),
            SignerDeviceId: deviceId,
            PopSignature: EncodeSignature(GoodSignatureMarker));

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.ResetIdentityAsync(
                userId, payload, AuditCtx, TestContext.Current.CancellationToken));

        Assert.Contains(
            PopFailureReason.ChallengeExpired,
            fixture.Metrics.PopFailures.Select(f => f.Reason));
    }

    [Fact]
    public async Task ResetIdentity_Fails_When_Challenge_Belongs_To_Other_User()
    {
        var (attackerId, victimId) = (UserId.New(), UserId.New());
        var attackerDevice = CreateEnrolledDevice(attackerId, out var attackerDeviceId);
        var victimChallenge = CreateChallenge(victimId);
        var fixture = BuildFixture(
            userId: attackerId,
            devices: [attackerDevice],
            challenges: [victimChallenge]);

        var payload = new IdentityResetPopPayload(
            ChallengeId: victimChallenge.Id,
            SignerDeviceId: attackerDeviceId,
            PopSignature: EncodeSignature(GoodSignatureMarker));

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.ResetIdentityAsync(
                attackerId, payload, AuditCtx, TestContext.Current.CancellationToken));

        Assert.Contains(
            PopFailureReason.ChallengeExpired,
            fixture.Metrics.PopFailures.Select(f => f.Reason));
    }

    [Fact]
    public async Task ResetIdentity_Fails_When_Signer_Not_Enrolled_And_Leaves_Challenge_Intact()
    {
        var userId = UserId.New();
        var device = CreateEnrolledDevice(userId, out _);
        var challenge = CreateChallenge(userId);
        var fixture = BuildFixture(
            userId: userId,
            devices: [device],
            challenges: [challenge]);

        var payload = new IdentityResetPopPayload(
            ChallengeId: challenge.Id,
            SignerDeviceId: "unknown-device",
            PopSignature: EncodeSignature(GoodSignatureMarker));

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.ResetIdentityAsync(
                userId, payload, AuditCtx, TestContext.Current.CancellationToken));

        Assert.Contains(
            PopFailureReason.SignerNotEnrolled,
            fixture.Metrics.PopFailures.Select(f => f.Reason));
    }

    [Fact]
    public async Task ResetIdentity_Requires_Payload_When_User_Has_Enrolled_Devices()
    {
        var userId = UserId.New();
        var device = CreateEnrolledDevice(userId, out _);
        var fixture = BuildFixture(
            userId: userId,
            devices: [device],
            challenges: []);

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.ResetIdentityAsync(
                userId, popPayload: null, AuditCtx, TestContext.Current.CancellationToken));

        Assert.Empty(fixture.Metrics.PopFailures);
        Assert.NotEmpty(
            await fixture.Devices.GetByUserIdAsync(userId, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ResetIdentity_Skips_Pop_When_User_Has_No_Enrolled_Devices()
    {
        var userId = UserId.New();
        var fixture = BuildFixture(
            userId: userId,
            devices: [],
            challenges: []);

        var result = await fixture.Service.ResetIdentityAsync(
            userId, popPayload: null, AuditCtx, TestContext.Current.CancellationToken);

        Assert.Empty(fixture.Metrics.PopFailures);
        Assert.Single(fixture.Audit.Entries);
        Assert.Equal(0, result.DevicesRemoved);
    }

    [Fact]
    public async Task ResetIdentity_HappyPath_Consumes_Challenge_And_Cascades()
    {
        var userId = UserId.New();
        var device = CreateEnrolledDevice(userId, out var deviceId);
        var challenge = CreateChallenge(userId);
        var fixture = BuildFixture(
            userId: userId,
            devices: [device],
            challenges: [challenge]);

        var payload = new IdentityResetPopPayload(
            ChallengeId: challenge.Id,
            SignerDeviceId: deviceId,
            PopSignature: EncodeSignature(GoodSignatureMarker));

        var result = await fixture.Service.ResetIdentityAsync(
            userId, payload, AuditCtx, TestContext.Current.CancellationToken);

        Assert.Empty(fixture.Metrics.PopFailures);
        Assert.True(fixture.Challenges.WasConsumed(challenge.Id));
        Assert.Equal(1, result.DevicesRemoved);


        Assert.Single(fixture.Audit.Entries);
        Assert.Empty(
            await fixture.Devices.GetByUserIdAsync(userId, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ResetIdentity_Gates_Owned_And_Recipient_Sessions_Before_Deleting_Blobs()
    {
        var userId = UserId.New();
        var device = CreateEnrolledDevice(userId, out var deviceId);
        var challenge = CreateChallenge(userId);
        var ownedSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            userId,
            "owned-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        ownedSession.ActivateHost("test-host");
        ownedSession.ReleaseHostSlot("test-host");
        var foreignOwnerId = UserId.New();
        var foreignSession = Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            1,
            Session.CurrentIncarnationProtocolVersion,
            foreignOwnerId,
            "foreign-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "foreign-secret");
        foreignSession.ActivateHost("foreign-host");
        foreignSession.ReleaseHostSlot("foreign-host");
        var blob = SessionKeyBlob.Create(
            foreignSession.Id,
            deviceId,
            [1, 2, 3],
            "foreign-owner-device",
            [4, 5, 6],
            1,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [7, 8, 9],
            issuedAtMs: 1_000);
        var fixture = BuildFixture(
            userId,
            devices: [device],
            challenges: [challenge],
            sessions: [ownedSession, foreignSession],
            blobs: [blob]);

        var result = await fixture.Service.ResetIdentityAsync(
            userId,
            new IdentityResetPopPayload(
                challenge.Id,
                deviceId,
                EncodeSignature(GoodSignatureMarker)),
            AuditCtx,
            TestContext.Current.CancellationToken);
        Assert.Equal(
            new[] { ownedSession.Id, foreignSession.Id }
                .OrderBy(sessionId => sessionId.Value),
            fixture.SessionEndAuthority.AcquiredSessionIds);
        var targetBySessionId = Assert.Single(fixture.Audit.Entries)
            .GetSessionTargets()
            .ToDictionary(target => target.SessionId);
        Assert.Equal(ownedSession.IncarnationId, targetBySessionId[ownedSession.Id].IncarnationId);
        Assert.Equal(foreignSession.IncarnationId, targetBySessionId[foreignSession.Id].IncarnationId);
    }

    [Fact]
    public async Task ResetIdentity_Does_Not_Fence_A_Republished_Foreign_Incarnation()
    {
        var userId = UserId.New();
        var device = CreateEnrolledDevice(userId, out var deviceId);
        device.Revoke(deviceId);
        var foreignOwner = UserId.New();
        var foreignSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            foreignOwner,
            "foreign-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "foreign-secret");
        foreignSession.ActivateHost("test-host");
        foreignSession.ReleaseHostSlot("test-host");
        foreignSession.End();
        var replacementIncarnationId = Guid.NewGuid();
        var replacementKeyGeneration = -1;
        var blob = SessionKeyBlob.Create(
            foreignSession.Id,
            deviceId,
            [1, 2, 3],
            "foreign-owner-device",
            [4, 5, 6],
            1,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [7, 8, 9],
            issuedAtMs: 1_000);
        var fixture = BuildFixture(
            userId,
            devices: [device],
            challenges: [],
            sessions: [foreignSession],
            blobs: [blob],
            onAffectedSessionDiscovery: blobRepository =>
            {
                foreignSession.Republish(
                    replacementIncarnationId,
                    foreignSession.IncarnationGeneration + 1,
                    foreignOwner,
                    "replacement-session",
                    SessionScope.Friends,
                    ToolKind.Terminal,
                    AccessLevel.View,
                    "replacement-secret",
                    roomId: null);
                blobRepository.RemoveForSession(foreignSession.Id);
                replacementKeyGeneration = foreignSession.CurrentKeyGeneration;
            });

        var result = await fixture.Service.ResetIdentityAsync(
            userId,
            popPayload: null,
            AuditCtx,
            TestContext.Current.CancellationToken);

        Assert.Equal(replacementIncarnationId, foreignSession.IncarnationId);
        Assert.Equal(replacementKeyGeneration, foreignSession.CurrentKeyGeneration);
        Assert.Equal(SessionStatus.Pending, foreignSession.Status);
    }

    [Fact]
    public async Task ResetIdentity_Closes_Foreign_Device_Queue_Before_Waiting_Action_Can_Dispatch()
    {
        var userId = UserId.New();
        var device = CreateEnrolledDevice(userId, out var deviceId);
        var challenge = CreateChallenge(userId);
        var foreignSession = Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            1,
            Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "foreign-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "foreign-secret");
        foreignSession.ActivateHost("foreign-host");
        foreignSession.ReleaseHostSlot("foreign-host");
        var foreignSessionId = foreignSession.Id;
        var blob = SessionKeyBlob.Create(
            foreignSessionId,
            deviceId,
            [1, 2, 3],
            "foreign-owner-device",
            [4, 5, 6],
            1,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [7, 8, 9],
            issuedAtMs: 1_000);
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var registry = new ConnectionRegistry();
        var runtime = runtimes.CreateRuntime(foreignSessionId);
        runtime.Host.SetStatus(SessionStatus.Live);
        runtime.Host.SetHostReady(true);
        var queues = broadcaster.GetOrCreateSession(foreignSessionId);
        queues.SetHostQueue("foreign-host");
        var participantQueue = queues.AddParticipantQueue(
            "reset-participant",
            AccessLevel.Suggest);
        registry.RegisterSharedParticipant(
            "reset-participant",
            userId,
            deviceId,
            foreignSessionId);

        var gate = new SessionLifecycleGate();
        var innerEnforcer = new IdentityResetRealtimeEnforcer(new RealtimeDeviceAccessEnforcementCore(
            registry,
            broadcaster,
            new UserEventBroadcaster(
                metrics,
                NullLogger<UserEventBroadcaster>.Instance)));
        var blockingEnforcer =
            new BlockingIdentityResetRealtimeEnforcer(innerEnforcer);
        var fixture = BuildFixture(
            userId,
            devices: [device],
            challenges: [challenge],
            sessions: [foreignSession],
            blobs: [blob],
            sessionLifecycleGate: gate,
            realtimeEnforcer: blockingEnforcer);

        var reset = fixture.Service.ResetIdentityAsync(
            userId,
            new IdentityResetPopPayload(
                challenge.Id,
                deviceId,
                EncodeSignature(GoodSignatureMarker)),
            AuditCtx,
            TestContext.Current.CancellationToken);
        await blockingEnforcer.EnforcementStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        var dispatchCalls = 0;
        var actionAuthority = new ParticipantActionAuthority(
            foreignSessionId,
            "reset-participant",
            userId,
            deviceId,
            new ParticipantAccessDecision(
                IsOwnerParticipant: false,
                AccessLevel: AccessLevel.Suggest),
            runtime,
            queues,
            participantQueue,
            runtimes,
            registry,
            broadcaster,
            gate);
        var dispatching = actionAuthority.TryDispatchAsync(
                SessionCapability.Suggest,
                () =>
                {
                    Interlocked.Increment(ref dispatchCalls);
                    return broadcaster.SendToHost(foreignSessionId, [1]);
                },
                TestContext.Current.CancellationToken)
            .AsTask();
        await WaitForReferenceCountAsync(gate, foreignSessionId, expected: 2);
        Assert.False(dispatching.IsCompleted);

        blockingEnforcer.AllowEnforcement.TrySetResult();
        await reset;
        var dispatch = await dispatching;

        Assert.False(dispatch.Authorized);
        Assert.Equal(0, dispatchCalls);
        Assert.Equal(
            QueueCompletionCause.AccessRevokedCascade,
            participantQueue.CompletionCause);
    }

    [Fact]
    public async Task ResetIdentity_TreatsExpiredSignedListAsNoActiveSigner()
    {
        var userId = UserId.New();
        var device = CreateEnrolledDevice(userId, out var deviceId);
        var expiredList = CreateDeviceList(
            userId,
            [deviceId],
            DateTimeOffset.UtcNow.AddSeconds(-1));
        var fixture = BuildFixture(
            userId,
            devices: [device],
            challenges: [],
            deviceLists: [expiredList]);

        var result = await fixture.Service.ResetIdentityAsync(
            userId,
            popPayload: null,
            AuditCtx,
            TestContext.Current.CancellationToken);

        Assert.Equal(1, result.DevicesRemoved);
        Assert.Single(fixture.Audit.Entries);
    }

    [Fact]
    public async Task ResetIdentity_CapturesOverrideOnlyPeerInDurableAudience()
    {
        var ownerId = UserId.New();
        var overridePeerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "override-audience",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        var accessOverrides = new FakeAccessOverrideRepository(
            SessionAccessOverride.Create(
                session.Id,
                overridePeerId,
                AccessLevel.View,
                ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow))
            .WithSessionOwner(session.Id, ownerId);
        var fixture = BuildFixture(
            ownerId,
            devices: [],
            challenges: [],
            sessions: [session],
            accessOverrides: accessOverrides);

        var result = await fixture.Service.ResetIdentityAsync(
            ownerId,
            popPayload: null,
            AuditCtx,
            TestContext.Current.CancellationToken);

        Assert.Equal(
            new[] { ownerId, overridePeerId }.OrderBy(userId => userId.Value),
            result.AudienceUserIds.OrderBy(userId => userId.Value));
        Assert.Equal(
            [overridePeerId.Value],
            Assert.Single(fixture.Audit.Entries).AudienceUserIds);
    }

    [Fact]
    public async Task ResetIdentity_CapturesHistoricalPeerWithoutActiveRelationship()
    {
        var ownerId = UserId.New();
        var historicalPeerId = UserId.New();
        var fixture = BuildFixture(
            ownerId,
            devices: [],
            challenges: [],
            identityExposures: new FakeIdentityExposureRepository(historicalPeerId));

        var result = await fixture.Service.ResetIdentityAsync(
            ownerId,
            popPayload: null,
            AuditCtx,
            TestContext.Current.CancellationToken);

        Assert.Equal(
            new[] { ownerId, historicalPeerId }.OrderBy(userId => userId.Value),
            result.AudienceUserIds.OrderBy(userId => userId.Value));
        Assert.Equal(
            [historicalPeerId.Value],
            Assert.Single(fixture.Audit.Entries).AudienceUserIds);
    }

    [Fact]
    public async Task ResetIdentity_AbortsBeforeCascade_WhenAudienceResolutionFails()
    {
        var userId = UserId.New();
        var fixture = BuildFixture(
            userId,
            devices: [],
            challenges: [],
            friendships: new ThrowingFriendshipRepository());

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            fixture.Service.ResetIdentityAsync(
                userId,
                popPayload: null,
                AuditCtx,
                TestContext.Current.CancellationToken));

        Assert.Empty(fixture.Audit.Entries);
        Assert.Empty(fixture.SessionEndAuthority.Projected);
    }

    [Fact]
    public async Task ResetIdentity_RechecksEnrolledDevices_AfterLifecycleLockAcquisition()
    {
        var userId = UserId.New();
        var enrolledDuringWait = CreateEnrolledDevice(userId, out _);
        var fixture = BuildFixture(
            userId,
            devices: [],
            challenges: [],
            onLifecycleLockAcquired: (repository, lists) =>
            {
                repository.Update(enrolledDuringWait);
                lists.AddAsync(
                        CreateDeviceList(userId, [enrolledDuringWait.DeviceId]),
                        CancellationToken.None)
                    .GetAwaiter()
                    .GetResult();
            });

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.ResetIdentityAsync(
                userId, popPayload: null, AuditCtx, TestContext.Current.CancellationToken));

        Assert.Empty(fixture.Audit.Entries);
        Assert.Single(await fixture.Devices.GetByUserIdAsync(
            userId, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ResetIdentity_RechecksSignerRevocation_AfterLifecycleLockAcquisition()
    {
        var userId = UserId.New();
        var signer = CreateEnrolledDevice(userId, out var signerDeviceId);
        var remainingDevice = CreateEnrolledDevice(userId, out _);
        var challenge = CreateChallenge(userId);
        var fixture = BuildFixture(
            userId,
            devices: [signer, remainingDevice],
            challenges: [challenge],
            onLifecycleLockAcquired: (_, _) => signer.Revoke(remainingDevice.DeviceId));

        var payload = new IdentityResetPopPayload(
            challenge.Id,
            signerDeviceId,
            EncodeSignature(GoodSignatureMarker));

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.ResetIdentityAsync(
                userId, payload, AuditCtx, TestContext.Current.CancellationToken));

        Assert.Contains(
            PopFailureReason.SignerNotEnrolled,
            fixture.Metrics.PopFailures.Select(f => f.Reason));
        Assert.Empty(fixture.Audit.Entries);
    }

    private sealed class PassThroughIdentityResetSessionResolver
        : IIdentityResetSessionResolver
    {
        public IReadOnlyList<IdentityResetSessionTarget>? Result { get; init; }
        public Func<IReadOnlyCollection<IdentityResetSessionTarget>,
            IReadOnlyList<IdentityResetSessionTarget>>? Resolve
        { get; init; }

        public Task<IReadOnlyList<IdentityResetSessionTarget>> GetCurrentTargetsAsync(
            IReadOnlyCollection<IdentityResetSessionTarget> targets,
            CancellationToken ct = default)
        {
            ct.ThrowIfCancellationRequested();
            return Task.FromResult(
                Resolve?.Invoke(targets)
                ?? Result
                ?? targets.ToList());
        }
    }

    private static UserDevice CreateEnrolledDevice(UserId owner, out string deviceId)
    {
        deviceId = $"device-{Guid.NewGuid():N}";
        var now = DateTimeOffset.UtcNow;
        var device = TestDeviceCertificate.CreateDevice(
            owner,
            deviceId,
            kemPublicKey: DummyKemKey,
            signingPublicKey: DummySignKey,
            deviceLabel: "Test device",
            signerDeviceId: deviceId,
            issuedAt: now.AddMinutes(-1),
            expiresAt: now.AddHours(1));
        return device;
    }

    private static UserDeviceList CreateDeviceList(
        UserId userId,
        IReadOnlyCollection<string> deviceIds,
        DateTimeOffset? expiresAt = null)
    {
        var now = DateTimeOffset.UtcNow;
        var signerDeviceId = deviceIds.FirstOrDefault() ?? "retired-device";
        return TestDeviceList.Create(
            userId,
            1,
            TestDeviceList.Entries(deviceIds.Select(
                deviceId => (deviceId, signerDeviceId))),
            signerDeviceId,
            [2],
            now.AddMinutes(-1).ToUnixTimeMilliseconds(),
            expiresAt?.ToUnixTimeMilliseconds());
    }

    private static DeviceRegistrationChallenge CreateChallenge(UserId userId)
    {
        var bytes = new byte[32];
        Random.Shared.NextBytes(bytes);
        return DeviceRegistrationChallenge.Create(userId, bytes, TimeSpan.FromMinutes(5));
    }

    private static string EncodeSignature(string marker)
        => Convert.ToBase64String(System.Text.Encoding.UTF8.GetBytes(marker));

    private static Fixture BuildFixture(
        UserId userId,
        UserDevice[] devices,
        DeviceRegistrationChallenge[] challenges,
        bool losePersistRace = false,
        Action<FakeUserDeviceRepository, FakeUserDeviceListRepository>?
            onLifecycleLockAcquired = null,
        Session[]? sessions = null,
        SessionId? dropSessionAfterTargetResolution = null,
        IFriendshipRepository? friendships = null,
        IRoomMemberRepository? rooms = null,
        IAccessOverrideRepository? accessOverrides = null,
        IIdentityExposureRepository? identityExposures = null,
        IReadOnlyList<UserDeviceList>? deviceLists = null,
        SessionKeyBlob[]? blobs = null,
        Action<FakeSessionKeyBlobRepository>? onAffectedSessionDiscovery = null,
        SessionLifecycleGate? sessionLifecycleGate = null,
        IIdentityResetRealtimeEnforcer? realtimeEnforcer = null,
        IIdentityResetDurabilityCoordinator? durabilityCoordinator = null)
    {
        foreach (var device in devices)
        {
            if (device.UserId != userId)
            {
                throw new InvalidOperationException(
                    $"Test device {device.DeviceId} belongs to {device.UserId}, not {userId}.");
            }
        }

        var devicesRepo = new FakeUserDeviceRepository(devices);
        var user = User.Create(
            userId,
            $"{userId.Value:N}@example.test",
            $"reset-{userId.Value:N}",
            "Reset user");
        var users = new FakeUserRepository(user);
        var effectiveDeviceLists = deviceLists
            ?? (devices.Length == 0
                ? []
                : [CreateDeviceList(
                    userId,
                    devices.Select(device => device.DeviceId).ToList())]);
        var lists = new FakeUserDeviceListRepository(effectiveDeviceLists.ToArray());
        var blobRepository = new FakeSessionKeyBlobRepository(blobs ?? []);
        var sessionArray = sessions ?? [];
        var sessionRepository = new FakeSessionRepository(
            sessionArray.FirstOrDefault(),
            sessionArray.Skip(1).ToArray())
        {
            DropAfterFirstBulkLockRead = dropSessionAfterTargetResolution,
        };
        friendships ??= new FakeFriendshipRepository();
        rooms ??= new FakeRoomMemberRepository();
        accessOverrides ??= new FakeAccessOverrideRepository();
        var challengesRepo = new FakeChallengeRepository(challenges)
        {
            SimulatePersistLostRace = losePersistRace,
        };
        var verifier = new MarkerPopVerifier(challengesRepo);
        var audit = new FakeIdentityResetAuditRepository();
        var unitOfWork = new FakeUnitOfWork();
        var liveTerminator = new LiveSessionTerminator(
            sessionRepository,
            blobRepository,
            unitOfWork,
            new FakeSessionEndMutationRepository());
        var metrics = new RecordingAuthMetrics();
        var popConsumer = new PopChallengeConsumer(challengesRepo, verifier, metrics);
        var popVerifier = new IdentityResetPopVerifier(popConsumer, metrics);
        var deviceLinks = new FakeDeviceLinkRequestRepository();
        var resetCascade = new IdentityResetCascade(
            devicesRepo,
            lists,
            deviceLinks,
            new FakeSemanticRelayLifecycleRepository(),
            blobRepository,
            sessionRepository,
            audit,
            unitOfWork,
            liveTerminator);
        var lifecycleLock = new CallbackLifecycleLock(
            () => onLifecycleLockAcquired?.Invoke(devicesRepo, lists));

        var sessionEndAuthority =
            new FakeSessionEndAuthority(sessionLifecycleGate);
        realtimeEnforcer ??=
            new RecordingIdentityResetRealtimeEnforcer(
                sessionEndAuthority,
                () => onAffectedSessionDiscovery?.Invoke(blobRepository));
        durabilityCoordinator ??= new FakeIdentityResetDurabilityCoordinator();
        var service = new IdentityLifecycleService(
            devicesRepo,
            lists,
            users,
            friendships,
            rooms,
            accessOverrides,
            identityExposures ?? new FakeIdentityExposureRepository(),
            popVerifier,
            resetCascade,
            unitOfWork,
            lifecycleLock,
            new FakeRecipientDeviceLifecycleLock(),
            sessionEndAuthority,
            realtimeEnforcer,
            durabilityCoordinator,
            NullLogger<IdentityLifecycleService>.Instance);

        return new Fixture(
            service,
            devicesRepo,
            deviceLinks,
            audit,
            metrics,
            challengesRepo,
            verifier,
            sessionEndAuthority,
            realtimeEnforcer);
    }

    private sealed class FakeIdentityExposureRepository(params UserId[] users)
        : IIdentityExposureRepository
    {
        private readonly IReadOnlyList<UserId> _users = users;

        public Task RecordAsync(
            UserId identityOwnerUserId,
            UserId recipientUserId,
            DateTimeOffset exposedAt,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task<IReadOnlyList<UserId>> GetHistoricalPeerUserIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            Task.FromResult(_users);

        public Task<IReadOnlyList<IdentityLifecycleProjection>> GetLifecycleSnapshotForRecipientAsync(
            UserId recipientUserId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<IdentityLifecycleProjection>>([]);
    }

    private sealed class FakeSemanticRelayLifecycleRepository
        : ISemanticRelayLifecycleRepository
    {
        public List<UserId> DeletedAccounts { get; } = [];
        public List<(UserId UserId, IReadOnlyCollection<string> DeviceIds)> DeletedDevices { get; } = [];

        public Task DeleteForDevicesAsync(
            UserId userId,
            IReadOnlyCollection<string> deviceIds,
            CancellationToken ct = default)
        {
            DeletedDevices.Add((userId, deviceIds));
            return Task.CompletedTask;
        }

        public Task DeleteForAccountAsync(UserId userId, CancellationToken ct = default)
        {
            DeletedAccounts.Add(userId);
            return Task.CompletedTask;
        }
    }

    private sealed class FakeDeviceLinkRequestRepository : IDeviceLinkRequestRepository
    {
        public List<UserId> InvalidatedUsers { get; } = [];

        public Task AddAsync(DeviceLinkRequest request, CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<DeviceLinkRequest?> GetByDeviceCodeAsync(
            string deviceCode,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<DeviceLinkRequest?> GetByUserCodeAsync(
            string userCode,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<bool> IsUserCodeAvailableAsync(
            string userCode,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<DeviceLinkRequest>> ListPendingForUserAsync(
            UserId userId,
            DateTimeOffset now,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public void Update(DeviceLinkRequest request) => throw new NotSupportedException();

        public Task<DeviceLinkCancelOutcome> CancelPendingAsync(
            string userCode,
            UserId userId,
            DateTimeOffset cancelledAt,
            CancellationToken ct = default) =>
            Task.FromResult(DeviceLinkCancelOutcome.NotFound);

        public Task<DeviceLinkAcknowledgeOutcome> AcknowledgeApprovedAsync(
            Guid requestId,
            UserId userId,
            string deviceId,
            DateTimeOffset now,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<int> InvalidateApprovedBeforeGenerationAsync(
            UserId userId,
            long committedGeneration,
            Guid? excludingRequestId,
            DateTimeOffset invalidatedAt,
            CancellationToken ct = default) =>
            Task.FromResult(0);

        public Task<int> InvalidateOutstandingForUserAsync(
            UserId userId,
            DateTimeOffset invalidatedAt,
            CancellationToken ct = default)
        {
            InvalidatedUsers.Add(userId);
            return Task.FromResult(0);
        }

        public Task<int> DeleteStaleAsync(
            DateTimeOffset now,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }

    private sealed record Fixture(
        IdentityLifecycleService Service,
        FakeUserDeviceRepository Devices,
        FakeDeviceLinkRequestRepository DeviceLinks,
        FakeIdentityResetAuditRepository Audit,
        RecordingAuthMetrics Metrics,
        FakeChallengeRepository Challenges,
        MarkerPopVerifier Verifier,
        FakeSessionEndAuthority SessionEndAuthority,
        IIdentityResetRealtimeEnforcer RealtimeEnforcer);

    private sealed class CallbackLifecycleLock(Action? onAcquired) : IUserLifecycleLock
    {
        private readonly Action? _onAcquired = onAcquired;

        public Task AcquireAsync(UserId userId, CancellationToken ct = default)
        {
            _onAcquired?.Invoke();
            return Task.CompletedTask;
        }
    }

    private sealed class FakeSessionEndAuthority(
        SessionLifecycleGate? lifecycleGate = null) : ISessionEndAuthority
    {
        private bool _leaseHeld;

        public bool LeaseHeld => _leaseHeld;
        public IReadOnlyList<SessionId> AcquiredSessionIds { get; private set; } = [];
        public List<CommittedSessionEnd> Projected { get; } = [];
        public List<IdentityResetEndedSession> Retired { get; } = [];
        public bool ProjectedBeforeLeaseRelease { get; private set; }

        public async ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default)
        {
            AcquiredSessionIds = [.. sessionIds];
            _leaseHeld = true;
            var inner = lifecycleGate is null
                ? null
                : await lifecycleGate.AcquireAsync(sessionIds, ct);
            return new NoopLease(this, inner);
        }

        public Task ProjectCommittedAsync(
            IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
            CancellationToken ct = default)
        {
            ProjectedBeforeLeaseRelease = _leaseHeld;
            Projected.AddRange(sessionEnds);
            return Task.CompletedTask;
        }

        public Task RetireEndedIncarnationAsync(
            SessionDiscoveryTarget endedSession,
            DateTimeOffset startedAt,
            CancellationToken ct = default)
        {
            Retired.Add(new IdentityResetEndedSession(endedSession, startedAt));
            ProjectedBeforeLeaseRelease = _leaseHeld;
            return Task.CompletedTask;
        }

        private sealed class NoopLease(
            FakeSessionEndAuthority owner,
            IAsyncDisposable? inner)
            : IAsyncDisposable
        {
            public async ValueTask DisposeAsync()
            {
                if (inner is not null)
                {
                    await inner.DisposeAsync();
                }
                owner._leaseHeld = false;
            }
        }
    }

    private sealed class RecordingIdentityResetRealtimeEnforcer(
        FakeSessionEndAuthority sessionEndAuthority,
        Action? onAffectedSessionDiscovery = null)
        : IIdentityResetRealtimeEnforcer
    {
        private bool _fenceHeld;

        public bool EnforcedBeforeLeaseRelease { get; private set; }
        public bool FenceHeldDuringEnforcement { get; private set; }
        public IReadOnlyList<SessionId> SessionsWithRevokedKeys { get; private set; } = [];

        public IDisposable FenceNewConnections(
            UserId userId,
            IReadOnlyCollection<string> removedDeviceIds)
        {
            _fenceHeld = true;
            return new CallbackDisposable(() => _fenceHeld = false);
        }

        public IReadOnlyList<SessionId> GetAffectedSessionIds(
            UserId userId,
            IReadOnlyCollection<string> removedDeviceIds,
            IReadOnlyCollection<SessionId> sessionsWithRevokedKeys)
        {
            onAffectedSessionDiscovery?.Invoke();
            return sessionsWithRevokedKeys
                .Distinct()
                .OrderBy(sessionId => sessionId.Value)
                .ToList();
        }

        public Task EnforceCommittedAsync(
            UserId userId,
            IReadOnlyCollection<string> removedDeviceIds,
            IReadOnlyCollection<SessionId> sessionsWithRevokedKeys,
            CancellationToken ct = default)
        {
            EnforcedBeforeLeaseRelease = sessionEndAuthority.LeaseHeld;
            FenceHeldDuringEnforcement = _fenceHeld;
            SessionsWithRevokedKeys = sessionsWithRevokedKeys.ToList();
            return Task.CompletedTask;
        }
    }

    private sealed class BlockingIdentityResetRealtimeEnforcer(
        IIdentityResetRealtimeEnforcer inner)
        : IIdentityResetRealtimeEnforcer
    {
        public TaskCompletionSource EnforcementStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource AllowEnforcement { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public IDisposable FenceNewConnections(
            UserId userId,
            IReadOnlyCollection<string> removedDeviceIds) =>
            inner.FenceNewConnections(userId, removedDeviceIds);

        public IReadOnlyList<SessionId> GetAffectedSessionIds(
            UserId userId,
            IReadOnlyCollection<string> removedDeviceIds,
            IReadOnlyCollection<SessionId> sessionsWithRevokedKeys) =>
            inner.GetAffectedSessionIds(
                userId,
                removedDeviceIds,
                sessionsWithRevokedKeys);

        public async Task EnforceCommittedAsync(
            UserId userId,
            IReadOnlyCollection<string> removedDeviceIds,
            IReadOnlyCollection<SessionId> sessionsWithRevokedKeys,
            CancellationToken ct = default)
        {
            EnforcementStarted.TrySetResult();
            await AllowEnforcement.Task.WaitAsync(ct);
            await inner.EnforceCommittedAsync(
                userId,
                removedDeviceIds,
                sessionsWithRevokedKeys,
                ct);
        }
    }

    private sealed class CallbackDisposable(Action dispose) : IDisposable
    {
        private Action? _dispose = dispose;

        public void Dispose() =>
            Interlocked.Exchange(ref _dispose, null)?.Invoke();
    }

    private static async Task WaitForReferenceCountAsync(
        SessionLifecycleGate gate,
        SessionId sessionId,
        int expected)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        while (gate.TestReferenceCount(sessionId) < expected)
        {
            await Task.Delay(1, timeout.Token);
        }
    }

    private sealed class ThrowingFriendshipRepository
        : IFriendshipRepository
    {
        public Task<IReadOnlyList<UserId>> GetFriendIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw new InvalidOperationException("Injected audience failure.");

        public Task<bool> AreFriendsAsync(
            UserId userA,
            UserId userB,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
        public Task<IReadOnlyList<Friendship>> GetPendingIncomingAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
        public Task<IReadOnlyList<Friendship>> GetPendingOutgoingAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
        public Task<Friendship?> GetAsync(
            UserId userA,
            UserId userB,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
        public Task AddAsync(
            Friendship friendship,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
        public void Remove(Friendship friendship) =>
            throw new NotSupportedException();
    }

    private sealed class FakeChallengeRepository(params DeviceRegistrationChallenge[] challenges) : IDeviceRegistrationChallengeRepository
    {
        private readonly Dictionary<Guid, DeviceRegistrationChallenge> _byId = challenges.ToDictionary(c => c.Id, c => c);

        private readonly HashSet<Guid> _consumed = [];

        public bool SimulatePersistLostRace { get; set; }

        public bool WasConsumed(Guid challengeId) => _consumed.Contains(challengeId);
        public bool HasConsumedChallenge => _consumed.Count > 0;

        public Task AddAsync(DeviceRegistrationChallenge challenge, CancellationToken ct = default)
        {
            _byId[challenge.Id] = challenge;
            return Task.CompletedTask;
        }

        public Task<DeviceRegistrationChallenge?> GetByIdAsync(Guid id, CancellationToken ct = default)
        {
            _byId.TryGetValue(id, out var challenge);
            return Task.FromResult(challenge);
        }

        public Task<int> DeleteExpiredAsync(
            DateTimeOffset now,
            int limit,
            CancellationToken ct = default) =>
            Task.FromResult(0);

        public Task<bool> TryConsumeAsync(DeviceRegistrationChallenge challenge, CancellationToken ct = default)
        {
            if (SimulatePersistLostRace)
            {
                SimulatePersistLostRace = false;
                return Task.FromResult(false);
            }

            if (!_byId.TryGetValue(challenge.Id, out var current)
                || !ReferenceEquals(current, challenge)
                || !challenge.IsValid(DateTimeOffset.UtcNow))
            {
                return Task.FromResult(false);
            }

            _byId.Remove(challenge.Id);
            _consumed.Add(challenge.Id);
            return Task.FromResult(true);
        }
    }

    private sealed class MarkerPopVerifier(FakeChallengeRepository challenges) : IPopSignatureVerifier
    {
        private readonly FakeChallengeRepository _challenges = challenges;

        public bool ChallengeConsumedAtVerifyTime { get; private set; }

        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature)
        {
            ChallengeConsumedAtVerifyTime = _challenges.HasConsumedChallenge;

            var marker = System.Text.Encoding.UTF8.GetString(signature);
            if (marker != GoodSignatureMarker && marker != BadSignatureMarker)
            {
                throw new InvalidOperationException(
                    $"Unexpected signature marker '{marker}'.");
            }

            if (message.Length < DomainTags.DevicePopV1.Length
                || !message[..DomainTags.DevicePopV1.Length]
                    .SequenceEqual(DomainTags.DevicePopV1))
            {
                throw new InvalidOperationException(
                    "PoP preimage missing or misaligned DevicePopV1 tag.");
            }

            return marker == GoodSignatureMarker;
        }
    }

    private sealed class FakeIdentityResetAuditRepository : IIdentityResetAuditRepository
    {
        public List<IdentityResetAuditEntry> Entries { get; } = [];

        public Task AddAsync(IdentityResetAuditEntry entry, CancellationToken ct = default)
        {
            Entries.Add(entry);
            return Task.CompletedTask;
        }
    }

    private sealed class FakeIdentityResetDurabilityCoordinator(
        IReadOnlyList<IdentityResetEnforcementWork>? pending = null)
        : IIdentityResetDurabilityCoordinator
    {
        private readonly IReadOnlyList<IdentityResetEnforcementWork> _pending =
            pending ?? [];

        public List<Guid> CompletedResetIds { get; } = [];
        public bool IsCurrent { get; init; } = true;
        public bool BlockUntilCancelled { get; init; }
        public bool AttemptWasCancelled { get; private set; }

        public Task<IdentityResetCommitReconciliation> ReconcileCommitAsync(
            Guid resetId,
            UserId userId,
            IReadOnlyCollection<string> expectedRemovedDeviceIds,
            CancellationToken ct = default) =>
            Task.FromResult(
                new IdentityResetCommitReconciliation(
                    IdentityResetCommitState.Committed));

        public Task<IReadOnlyList<IdentityResetEnforcementWork>>
            GetPendingEnforcementAsync(
                int limit,
                CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<IdentityResetEnforcementWork>>(
                _pending.Take(limit).ToList());

        public async Task<bool> ExecuteIfCurrentAsync(
            IdentityResetEnforcementWork work,
            Func<CancellationToken, Task> enforce,
            CancellationToken ct = default)
        {
            if (!IsCurrent)
            {
                return false;
            }
            if (BlockUntilCancelled)
            {
                try
                {
                    await Task.Delay(Timeout.InfiniteTimeSpan, ct);
                }
                catch (OperationCanceledException) when (ct.IsCancellationRequested)
                {
                    AttemptWasCancelled = true;
                    throw;
                }
            }
            await enforce(ct);
            return true;
        }

        public Task CompleteEnforcementAsync(
            Guid resetId,
            CancellationToken ct = default)
        {
            CompletedResetIds.Add(resetId);
            return Task.CompletedTask;
        }
    }

    private sealed class RecordingAuthMetrics : IAuthMetrics
    {
        public List<(PopFailureReason Reason, string Endpoint)> PopFailures { get; } = [];

        public void RecordPopFailure(PopFailureReason reason, string endpoint)
            => PopFailures.Add((reason, endpoint));
    }
}
