using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class SharingAndRoomServiceTests
{
    [Fact]
    public async Task GrantAccessAsync_Exact_Retry_Does_Not_Reapply_Mutation()
    {
        var ownerUserId = UserId.New();
        var actorUserId = UserId.New();
        var mutationId = Guid.CreateVersion7();
        var expiresAt = DateTimeOffset.UtcNow.AddHours(1);
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId, "session", SessionScope.Friends,
            ToolKind.ClaudeCode, AccessLevel.View, "secret");
        var overrides = new LocalFakeAccessOverrideRepository();
        var audit = new FakeAccessOverrideAuditRepository();
        var mutations = new FakeSessionAccessMutationRepository();
        var unitOfWork = new LocalFakeUnitOfWork();
        var service = new SharingService(
            new LocalFakeSessionRepository(session),
            new LocalFakeUserRepository(),
            overrides,
            audit,
            mutations,
            new FakeUserLifecycleLock(),
            new FakeSessionEndAuthority(),
            unitOfWork);

        await using (await service.GrantAccessAsync(
            session.Id,
            session.IncarnationId,
            mutationId,
            actorUserId,
            AccessLevel.Inject,
            ownerUserId,
            expiresAt,
            TestContext.Current.CancellationToken)) { }
        await using (await service.GrantAccessAsync(
            session.Id,
            session.IncarnationId,
            mutationId,
            actorUserId,
            AccessLevel.Inject,
            ownerUserId,
            expiresAt,
            TestContext.Current.CancellationToken)) { }

        Assert.Equal(1, overrides.AddCalls);
        Assert.Equal(0, overrides.UpdateCalls);
        Assert.Single(audit.Entries);
        Assert.Equal(1, unitOfWork.SaveCalls);
        Assert.Equal(2, unitOfWork.CommitCalls);
    }

    [Fact]
    public async Task GrantAccessAsync_Rejects_Unknown_Grantee_Before_Persistence()
    {
        var ownerUserId = UserId.New();
        var unknownUserId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId,
            "session",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.View,
            "secret");
        var overrides = new LocalFakeAccessOverrideRepository();
        var audit = new FakeAccessOverrideAuditRepository();
        var unitOfWork = new LocalFakeUnitOfWork();
        var authority = new RecordingSessionEndAuthority();
        var service = new SharingService(
            new LocalFakeSessionRepository(session),
            new LocalFakeUserRepository(unknownUserId),
            overrides,
            audit,
            new FakeSessionAccessMutationRepository(),
            new FakeUserLifecycleLock(),
            authority,
            unitOfWork);

        await Assert.ThrowsAsync<NotFoundException>(() => service.GrantAccessAsync(
            session.Id,
            session.IncarnationId,
            Guid.CreateVersion7(),
            unknownUserId,
            AccessLevel.View,
            ownerUserId,
            DateTimeOffset.UtcNow.AddHours(1),
            TestContext.Current.CancellationToken));

        Assert.Equal(0, overrides.AddCalls);
        Assert.Empty(audit.Entries);
        Assert.Equal(0, unitOfWork.SaveCalls);
        Assert.False(authority.LeaseHeld);
    }

    [Fact]
    public async Task GrantAccessAsync_Mutation_Id_Reuse_For_Different_Target_Conflicts()
    {
        var ownerUserId = UserId.New();
        var actorUserId = UserId.New();
        var mutationId = Guid.CreateVersion7();
        var expiresAt = DateTimeOffset.UtcNow.AddHours(1);
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId, "session", SessionScope.Friends,
            ToolKind.ClaudeCode, AccessLevel.View, "secret");
        var overrides = new LocalFakeAccessOverrideRepository();
        var audit = new FakeAccessOverrideAuditRepository();
        var unitOfWork = new LocalFakeUnitOfWork();
        var service = new SharingService(
            new LocalFakeSessionRepository(session),
            new LocalFakeUserRepository(),
            overrides,
            audit,
            new FakeSessionAccessMutationRepository(),
            new FakeUserLifecycleLock(),
            new FakeSessionEndAuthority(),
            unitOfWork);

        await using (await service.GrantAccessAsync(
            session.Id,
            session.IncarnationId,
            mutationId,
            actorUserId,
            AccessLevel.View,
            ownerUserId,
            expiresAt,
            TestContext.Current.CancellationToken)) { }

        var conflict = await Assert.ThrowsAsync<SessionAccessMutationTargetConflictException>(() =>
            service.GrantAccessAsync(
                session.Id,
                session.IncarnationId,
                mutationId,
                actorUserId,
                AccessLevel.Inject,
                ownerUserId,
                expiresAt,
                TestContext.Current.CancellationToken));

        Assert.Equal("SESSION_ACCESS_MUTATION_TARGET_CONFLICT", conflict.Code);
        Assert.Equal(1, overrides.AddCalls);
        Assert.Equal(0, overrides.UpdateCalls);
        Assert.Single(audit.Entries);
        Assert.Equal(1, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task RevokeAccessAsync_Exact_Retry_Does_Not_Reapply_Mutation()
    {
        var ownerUserId = UserId.New();
        var actorUserId = UserId.New();
        var mutationId = Guid.CreateVersion7();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId, "session", SessionScope.Friends,
            ToolKind.ClaudeCode, AccessLevel.View, "secret");
        var accessOverride = SessionAccessOverride.Create(
            session.Id,
            actorUserId,
            AccessLevel.Inject,
            ownerUserId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new LocalFakeAccessOverrideRepository(accessOverride);
        var audit = new FakeAccessOverrideAuditRepository();
        var unitOfWork = new LocalFakeUnitOfWork();
        var service = new SharingService(
            new LocalFakeSessionRepository(session),
            new LocalFakeUserRepository(),
            overrides,
            audit,
            new FakeSessionAccessMutationRepository(),
            new FakeUserLifecycleLock(),
            new FakeSessionEndAuthority(),
            unitOfWork);

        await using (await service.RevokeAccessAsync(
            session.Id,
            session.IncarnationId,
            mutationId,
            actorUserId,
            ownerUserId,
            TestContext.Current.CancellationToken)) { }
        await using (await service.RevokeAccessAsync(
            session.Id,
            session.IncarnationId,
            mutationId,
            actorUserId,
            ownerUserId,
            TestContext.Current.CancellationToken)) { }

        Assert.False(accessOverride.IsActiveAt(DateTimeOffset.UtcNow));
        Assert.Equal(1, overrides.UpdateCalls);
        Assert.Single(audit.Entries);
        Assert.Equal(1, unitOfWork.SaveCalls);
        Assert.Equal(2, unitOfWork.CommitCalls);
    }

    [Fact]
    public async Task GrantAccessAsync_Updates_Existing_Override_In_Place()
    {
        var ownerUserId = UserId.New();
        var actorUserId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId, "session", SessionScope.Friends,
            ToolKind.ClaudeCode, AccessLevel.View, "secret");
        var existingOverride = SessionAccessOverride.Create(
            session.Id, actorUserId, AccessLevel.View, ownerUserId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new LocalFakeAccessOverrideRepository(existingOverride);
        var unitOfWork = new LocalFakeUnitOfWork();
        var service = new SharingService(
            new LocalFakeSessionRepository(session),
            new LocalFakeUserRepository(),
            overrides,
            new FakeAccessOverrideAuditRepository(),
            new FakeSessionAccessMutationRepository(),
            new FakeUserLifecycleLock(),
            new FakeSessionEndAuthority(),
            unitOfWork);

        await service.GrantAccessAsync(
            session.Id,
            session.IncarnationId,
            Guid.CreateVersion7(),
            actorUserId,
            AccessLevel.Inject,
            ownerUserId,
            expiresAt: DateTimeOffset.UtcNow.AddHours(1),
            TestContext.Current.CancellationToken);

        Assert.Equal(0, overrides.AddCalls);
        Assert.Equal(1, overrides.UpdateCalls);
        Assert.True(existingOverride.IsActiveAt(DateTimeOffset.UtcNow));
        Assert.Equal(AccessLevel.Inject, existingOverride.AccessLevel);
        Assert.Equal(ownerUserId, existingOverride.GrantedByUserId);
        Assert.Equal(1, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task GrantAccessAsync_Reactivates_Revoked_Override_In_Place()
    {
        var ownerUserId = UserId.New();
        var actorUserId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId, "session", SessionScope.Friends,
            ToolKind.ClaudeCode, AccessLevel.View, "secret");
        var existingOverride = SessionAccessOverride.Create(
            session.Id, actorUserId, AccessLevel.View, ownerUserId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        existingOverride.Revoke();
        var overrides = new LocalFakeAccessOverrideRepository(existingOverride);
        var unitOfWork = new LocalFakeUnitOfWork();
        var service = new SharingService(
            new LocalFakeSessionRepository(session),
            new LocalFakeUserRepository(),
            overrides,
            new FakeAccessOverrideAuditRepository(),
            new FakeSessionAccessMutationRepository(),
            new FakeUserLifecycleLock(),
            new FakeSessionEndAuthority(),
            unitOfWork);

        await service.GrantAccessAsync(
            session.Id,
            session.IncarnationId,
            Guid.CreateVersion7(),
            actorUserId,
            AccessLevel.Suggest,
            ownerUserId,
            expiresAt: DateTimeOffset.UtcNow.AddHours(1),
            TestContext.Current.CancellationToken);

        Assert.Equal(0, overrides.AddCalls);
        Assert.Equal(1, overrides.UpdateCalls);
        Assert.True(existingOverride.IsActiveAt(DateTimeOffset.UtcNow));
        Assert.Equal(AccessLevel.Suggest, existingOverride.AccessLevel);
        Assert.Equal(1, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task GrantAccess_Does_Not_Authorize_Republished_Incarnation()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "old-sharing-incarnation",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        var overrides = new LocalFakeAccessOverrideRepository();
        var authority = new BlockingSessionEndAuthority();
        var service = new SharingService(
            new LocalFakeSessionRepository(session),
            new LocalFakeUserRepository(),
            overrides,
            new FakeAccessOverrideAuditRepository(),
            new FakeSessionAccessMutationRepository(),
            new FakeUserLifecycleLock(),
            authority,
            new LocalFakeUnitOfWork());

        var granting = service.GrantAccessAsync(
            session.Id,
            session.IncarnationId,
            Guid.CreateVersion7(),
            actorId,
            AccessLevel.View,
            ownerId,
            expiresAt: DateTimeOffset.UtcNow.AddHours(1),
            TestContext.Current.CancellationToken);
        await authority.AcquireStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        session.End();
        await Task.Delay(1, TestContext.Current.CancellationToken);
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "replacement-sharing-incarnation",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "new-secret",
            roomId: null);
        authority.Release();

        await Assert.ThrowsAsync<NotFoundException>(() => granting);
        Assert.Equal(0, overrides.AddCalls);
    }

    [Fact]
    public async Task RevokeAccess_Does_Not_Mutate_Republished_Incarnation()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "old-sharing-incarnation",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        var staleIncarnationId = session.IncarnationId;
        var existingOverride = SessionAccessOverride.Create(
            session.Id,
            actorId,
            AccessLevel.View,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new LocalFakeAccessOverrideRepository(existingOverride);
        session.End();
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "replacement-sharing-incarnation",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "new-secret",
            roomId: null);
        var service = new SharingService(
            new LocalFakeSessionRepository(session),
            new LocalFakeUserRepository(),
            overrides,
            new FakeAccessOverrideAuditRepository(),
            new FakeSessionAccessMutationRepository(),
            new FakeUserLifecycleLock(),
            new FakeSessionEndAuthority(),
            new LocalFakeUnitOfWork());

        await Assert.ThrowsAsync<NotFoundException>(() =>
            service.RevokeAccessAsync(
                session.Id,
                staleIncarnationId,
                Guid.CreateVersion7(),
                actorId,
                ownerId,
                TestContext.Current.CancellationToken));

        Assert.True(existingOverride.IsActiveAt(DateTimeOffset.UtcNow));
    }

    [Fact]
    public async Task SharingMutation_Retains_Lifecycle_Lease_Until_Fanout_Disposes_Result()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "lease-through-fanout",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        var authority = new RecordingSessionEndAuthority();
        var service = new SharingService(
            new LocalFakeSessionRepository(session),
            new LocalFakeUserRepository(),
            new LocalFakeAccessOverrideRepository(),
            new FakeAccessOverrideAuditRepository(),
            new FakeSessionAccessMutationRepository(),
            new FakeUserLifecycleLock(),
            authority,
            new LocalFakeUnitOfWork());

        var mutation = await service.GrantAccessAsync(
            session.Id,
            session.IncarnationId,
            Guid.CreateVersion7(),
            actorId,
            AccessLevel.View,
            ownerId,
            expiresAt: DateTimeOffset.UtcNow.AddHours(1),
            TestContext.Current.CancellationToken);

        Assert.True(authority.LeaseHeld);
        await mutation.DisposeAsync();
        Assert.False(authority.LeaseHeld);
    }

    [Fact]
    public async Task RemoveMemberAsync_Rejects_Owner_Self_Removal()
    {
        var ownerUserId = UserId.New();
        var room = TestRoom(ownerUserId);
        var ownerMember = DomainFixtureHydrator.RoomMember(room.Id, ownerUserId);
        var members = new LocalFakeRoomMemberRepository(ownerMember);
        var rosterTransitions = new RecordingRoomRosterTransitionRepository();
        var service = RoomServiceFor(
            room,
            members,
            new LocalFakeUserRepository(),
            rosterTransitions,
            new LocalFakeUnitOfWork());

        var error = await Assert.ThrowsAsync<PolicyViolationException>(() =>
            service.RemoveMemberWithLifecycleHeldAsync(
                room.Id,
                ownerUserId,
                ownerUserId,
                2,
                [3],
                [4],
                "owner-device",
                TestContext.Current.CancellationToken));

        Assert.Equal("The room owner cannot remove themselves.", error.Message);
        Assert.True(ownerMember.IsActive);
        Assert.Empty(rosterTransitions.Added);
    }

    [Fact]
    public async Task CreateRoom_Retry_With_Different_Fingerprint_Conflicts()
    {
        var ownerUserId = UserId.New();
        var room = TestRoom(ownerUserId);
        var members = new LocalFakeRoomMemberRepository();
        var unitOfWork = new LocalFakeUnitOfWork();
        var rosterTransitions = new RecordingRoomRosterTransitionRepository();
        var service = RoomServiceFor(
            room,
            members,
            new LocalFakeUserRepository(),
            rosterTransitions,
            unitOfWork,
            new FakeRoomLifecycleLock());

        await Assert.ThrowsAsync<ConflictException>(() => service.CreateAsync(
            room.Id,
            ownerUserId,
            "different-name-is-not-an-idempotency-proof",
            "different-slug",
            1,
            [9],
            [9],
            "different-device",
            TestContext.Current.CancellationToken));

        Assert.Equal(0, members.AddCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
        Assert.Empty(rosterTransitions.Added);
    }

    [Fact]
    public async Task RemoveMemberAsync_Rejects_NonOwner_Self_Removal()
    {
        var ownerUserId = UserId.New();
        var memberUserId = UserId.New();
        var room = TestRoom(ownerUserId);
        var member = DomainFixtureHydrator.RoomMember(room.Id, memberUserId, RoomRole.Member);
        var members = new LocalFakeRoomMemberRepository(member);
        var unitOfWork = new LocalFakeUnitOfWork();
        var rosterTransitions = new RecordingRoomRosterTransitionRepository();
        var service = RoomServiceFor(
            room,
            members,
            new LocalFakeUserRepository(),
            rosterTransitions,
            unitOfWork,
            new FakeRoomLifecycleLock());

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            service.RemoveMemberWithLifecycleHeldAsync(
                room.Id,
                memberUserId,
                memberUserId,
                2,
                [3],
                [4],
                "owner-device",
                TestContext.Current.CancellationToken));

        Assert.True(member.IsActive);
        Assert.Equal(0, unitOfWork.SaveCalls);
        Assert.Empty(rosterTransitions.Added);
    }

    [Fact]
    public async Task RemoveMemberAsync_Rejects_NonOwner_Removing_Other_Member()
    {
        var ownerUserId = UserId.New();
        var memberA = UserId.New();
        var memberB = UserId.New();
        var room = TestRoom(ownerUserId);
        var b = DomainFixtureHydrator.RoomMember(room.Id, memberB, RoomRole.Member);
        var members = new LocalFakeRoomMemberRepository(b);
        var rosterTransitions = new RecordingRoomRosterTransitionRepository();
        var service = RoomServiceFor(
            room,
            members,
            new LocalFakeUserRepository(),
            rosterTransitions,
            new LocalFakeUnitOfWork());

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            service.RemoveMemberWithLifecycleHeldAsync(
                room.Id,
                memberA,
                memberB,
                2,
                [3],
                [4],
                "owner-device",
                TestContext.Current.CancellationToken));

        Assert.True(b.IsActive);
        Assert.Empty(rosterTransitions.Added);
    }

    [Fact]
    public async Task RemoveMemberAsync_Accepts_Roster_That_Excludes_Removed_Member()
    {
        var ownerUserId = UserId.New();
        var memberUserId = UserId.New();
        var room = TestRoom(ownerUserId);
        var member = DomainFixtureHydrator.RoomMember(room.Id, memberUserId, RoomRole.Member);
        var members = new LocalFakeRoomMemberRepository(member);
        var rosterTransitions = new RecordingRoomRosterTransitionRepository();
        var service = RoomServiceFor(
            room,
            members,
            new LocalFakeUserRepository(),
            rosterTransitions,
            new LocalFakeUnitOfWork());

        await service.RemoveMemberWithLifecycleHeldAsync(
            room.Id,
            ownerUserId,
            memberUserId,
            2,
            RoomRosterTestData.Build(room, 2, [ownerUserId]),
            [4],
            "owner-device",
            TestContext.Current.CancellationToken);

        Assert.False(member.IsActive);
        var transition = Assert.Single(rosterTransitions.Added);
        Assert.Equal(room.Id, transition.RoomId);
        Assert.Equal(2, transition.Generation);
        Assert.Null(transition.AdmissionInvitationId);
    }

    [Fact]
    public async Task MembershipWorkflow_Holds_Session_Authority_Through_Committed_End_Projection()
    {
        var ownerUserId = UserId.New();
        var removedUserId = UserId.New();
        var room = TestRoom(ownerUserId);
        var member = DomainFixtureHydrator.RoomMember(room.Id, removedUserId, RoomRole.Member);
        var members = new LocalFakeRoomMemberRepository(member);
        var rosterTransitions = new RecordingRoomRosterTransitionRepository();
        var unitOfWork = new LocalFakeUnitOfWork();
        var authority = new RecordingSessionEndAuthority();
        var roomLifecycleLock = new RecordingRoomLifecycleLock();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            removedUserId,
            "removed-member-session",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            room.Id);
        var sessions = new WorkflowSessionRepository(
            session,
            authority,
            roomLifecycleLock);
        var receipts = new LocalFakeRoomMutationReceiptRepository();
        var workflow = new RoomMembershipWorkflowService(
            RoomServiceFor(
                room,
                members,
                new LocalFakeUserRepository(),
                rosterTransitions,
                unitOfWork),
            new SessionAccessOverrideRevoker(
                sessions,
                new LocalFakeAccessOverrideRepository(),
                new FakeSessionKeyBlobRepository()),
            new RecordingRoomMemberAuditRepository(),
            unitOfWork,
            roomLifecycleLock,
            authority,
            receipts);

        var requestId = Guid.CreateVersion7();
        var rosterBody = RoomRosterTestData.Build(room, 2, [ownerUserId]);
        var result = await workflow.RemoveMemberIdempotentlyAsync(
            requestId,
            room.Id,
            ownerUserId,
            removedUserId,
            2,
            rosterBody,
            [4],
            "owner-device",
            new RequestAuditContext("127.0.0.1", "xunit"),
            TestContext.Current.CancellationToken);

        Assert.Equal([session.Id], authority.AcquiredSessionIds);
        Assert.True(sessions.EnumeratedWhileRoomLockHeld);
        Assert.True(sessions.LockedWhileAuthorityHeld);
        Assert.True(authority.ProjectedBeforeLeaseRelease);
        var projected = Assert.Single(authority.Projected);
        Assert.Equal(session.Id, projected.Transition.SharingState?.SessionId);
        Assert.Equal(room.Id, projected.Transition.SharingState?.RoomId);
        Assert.Equal(CommittedSessionEndReason.AccessRevoked, projected.Reason);
        Assert.Equal(SessionStatus.Ended, session.Status);
        Assert.True(Assert.Single(result.AffectedSessions).EndedByRemoval);
        Assert.True(authority.LeaseHeld);
        await result.DisposeAsync();
        Assert.False(authority.LeaseHeld);

        var laterSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId,
            "later-room-session",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "later-secret",
            room.Id);
        sessions.Add(laterSession);
        var laterSessionStatus = laterSession.Status;
        var savesBeforeReplay = unitOfWork.SaveCalls;
        workflow = new RoomMembershipWorkflowService(
            RoomServiceFor(
                room,
                members,
                new LocalFakeUserRepository(),
                rosterTransitions,
                unitOfWork),
            new SessionAccessOverrideRevoker(
                sessions,
                new LocalFakeAccessOverrideRepository(),
                new FakeSessionKeyBlobRepository()),
            new RecordingRoomMemberAuditRepository(),
            unitOfWork,
            roomLifecycleLock,
            authority,
            receipts);

        var replay = await workflow.RemoveMemberIdempotentlyAsync(
            requestId,
            room.Id,
            ownerUserId,
            removedUserId,
            2,
            rosterBody,
            [4],
            "owner-device",
            new RequestAuditContext("127.0.0.1", "xunit"),
            TestContext.Current.CancellationToken);

        Assert.True(replay.IsDuplicate);
        var replayed = Assert.Single(replay.AffectedSessions);
        Assert.Equal(session.Id, replayed.SessionId);
        Assert.Equal(session.IncarnationId, replayed.IncarnationId);
        Assert.True(replayed.EndedByRemoval);
        Assert.Equal([session.Id], authority.AcquiredSessionIds);
        Assert.Equal(laterSessionStatus, laterSession.Status);
        Assert.DoesNotContain(
            replay.AffectedSessions,
            affected => affected.SessionId == laterSession.Id);
        Assert.Equal(2, authority.Projected.Count);
        Assert.Equal(
            room.Id,
            authority.Projected[1].Transition.SharingState?.RoomId);
        Assert.Equal(savesBeforeReplay, unitOfWork.SaveCalls);
        Assert.Equal(2, unitOfWork.CommitCalls);
        await replay.DisposeAsync();

        await Assert.ThrowsAsync<RoomMutationReceiptTargetConflictException>(() =>
            workflow.RemoveMemberIdempotentlyAsync(
                requestId,
                room.Id,
                ownerUserId,
                UserId.New(),
                2,
                rosterBody,
                [4],
                "owner-device",
                new RequestAuditContext("127.0.0.1", "xunit"),
                TestContext.Current.CancellationToken));
    }

    private static Room TestRoom(UserId ownerUserId) =>
        Room.Create(
            RoomId.From(Guid.NewGuid()),
            ownerUserId,
            "Room",
            "room",
            1,
            [1],
            [2],
            "owner-device");

    private static RoomService RoomServiceFor(
        Room room,
        IRoomMemberRepository members,
        IUserRepository users,
        IRoomRosterTransitionRepository rosterTransitions,
        IUnitOfWork unitOfWork,
        IRoomLifecycleLock? roomLifecycleLock = null)
    {
        var device = TestDeviceCertificate.CreateDevice(
            room.OwnerUserId,
            "owner-device",
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Owner device",
            signerDeviceId: "owner-device",
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: null);
        var list = TestDeviceList.Create(
            room.OwnerUserId,
            1,
            """[{"deviceId":"owner-device","signerDeviceId":"owner-device"}]""",
            "owner-device",
            [2],
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            null);
        return new RoomService(
            new LocalFakeRoomRepository(room),
            members,
            users,
            new RoomRosterVerifier(
                new FakeUserDeviceRepository(device),
                new FakeUserDeviceListRepository(list),
                new AlwaysValidSignatureVerifier(),
                TimeProvider.System),
            rosterTransitions,
            unitOfWork,
            roomLifecycleLock ?? new FakeRoomLifecycleLock());
    }


    [Fact]
    public async Task GetActiveGrantsAsync_Lists_Active_Grants_For_Owner()
    {
        var ownerUserId = UserId.New();
        var actorUserId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId, "session", SessionScope.Friends,
            ToolKind.ClaudeCode, AccessLevel.View, "secret");
        var grant = SessionAccessOverride.Create(
            session.Id, actorUserId, AccessLevel.Suggest, ownerUserId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var service = new SessionAccessQueryService(
            new LocalFakeSessionRepository(session),
            new LocalFakeAccessOverrideRepository(grant),
            new LocalFakeUserRepository());

        var result = await service.GetActiveGrantsAsync(
            session.Id,
            session.IncarnationId,
            ownerUserId,
            TestContext.Current.CancellationToken);

        Assert.Equal(session.IncarnationId, result.IncarnationId);
        var entry = Assert.Single(result.Grants);
        Assert.Equal(actorUserId.Value, entry.ActorUserId);
        Assert.Equal(AccessLevel.Suggest, entry.AccessLevel);
    }

    [Fact]
    public async Task GetActiveGrantsAsync_Rejects_Non_Owner()
    {
        var ownerUserId = UserId.New();
        var strangerUserId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId, "session", SessionScope.Friends,
            ToolKind.ClaudeCode, AccessLevel.View, "secret");
        var service = new SessionAccessQueryService(
            new LocalFakeSessionRepository(session),
            new LocalFakeAccessOverrideRepository(),
            new LocalFakeUserRepository());

        await Assert.ThrowsAsync<NotFoundException>(() =>
            service.GetActiveGrantsAsync(
                session.Id,
                session.IncarnationId,
                strangerUserId,
                TestContext.Current.CancellationToken));
    }

    private sealed class LocalFakeAccessOverrideRepository(SessionAccessOverride? accessOverride = null) : IAccessOverrideRepository
    {
        private SessionAccessOverride? _override = accessOverride;

        public int AddCalls { get; private set; }
        public int UpdateCalls { get; private set; }

        public Task<SessionAccessOverride?> GetAsync(
            SessionId sessionId, UserId actorUserId, CancellationToken ct = default)
            => Task.FromResult(Matches(sessionId, actorUserId) ? _override : null);

        public Task<SessionAccessOverride?> GetActiveAsync(
            SessionId sessionId, UserId actorUserId, CancellationToken ct = default)
            => Task.FromResult(Matches(sessionId, actorUserId) && _override?.IsActiveAt(DateTimeOffset.UtcNow) == true ? _override : null);

        public Task<IReadOnlyList<SessionAccessOverride>> GetActiveForActorAsync(
            UserId actorUserId, IReadOnlyList<SessionId> sessionIds, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<SessionAccessOverride>>(
                _override is not null && _override.IsActiveAt(DateTimeOffset.UtcNow) && _override.ActorUserId == actorUserId
                    && sessionIds.Contains(_override.SessionId) ? [_override] : []);

        public Task<IReadOnlyList<SessionAccessOverride>> GetActiveBySessionAsync(
            SessionId sessionId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<SessionAccessOverride>>(
                _override is not null && _override.IsActiveAt(DateTimeOffset.UtcNow) && _override.SessionId == sessionId ? [_override] : []);

        public Task<IReadOnlyList<SessionAccessOverride>> GetUnrevokedBySessionAsync(
            SessionId sessionId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SessionAccessOverride>>(
                _override is not null
                    && _override.RevokedAt is null
                    && _override.SessionId == sessionId
                        ? [_override]
                        : []);

        public Task AddAsync(SessionAccessOverride accessOverride, CancellationToken ct = default)
        { AddCalls++; _override = accessOverride; return Task.CompletedTask; }

        public Task UpdateAsync(SessionAccessOverride accessOverride, CancellationToken ct = default)
        { UpdateCalls++; _override = accessOverride; return Task.CompletedTask; }

        public Task UpdateRangeAsync(
            IReadOnlyCollection<SessionAccessOverride> accessOverrides,
            CancellationToken ct = default)
        {
            foreach (var updated in accessOverrides)
            {
                UpdateCalls++;
                _override = updated;
            }

            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<UserId>> GetActiveRelatedUserIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<bool> HasActiveRelationshipAsync(UserId userA, UserId userB, CancellationToken ct = default)
            => Task.FromResult(false);

        public Task<IReadOnlyList<SessionAccessOverride>> GetExpiredUnrevokedAsync(
            DateTimeOffset now,
            int limit,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<bool> TryRevokeExpiredAsync(
            SessionId sessionId,
            UserId actorUserId,
            DateTimeOffset observedExpiry,
            DateTimeOffset revokedAt,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        private bool Matches(SessionId sessionId, UserId actorUserId)
            => _override is not null && _override.SessionId == sessionId && _override.ActorUserId == actorUserId;
    }

    private sealed class LocalFakeSessionRepository(Session? session = null)
        : SessionRepositoryStub
    {
        public override Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)
            => Task.FromResult(session?.Id == id ? session : null);
        public override Task<Session?> GetByIdForUpdateAsync(
            SessionId id,
            CancellationToken ct = default) =>
            GetByIdAsync(id, ct);
        public override Task AddAsync(Session s, CancellationToken ct = default) => Task.CompletedTask;
        public override Task UpdateAsync(Session s, CancellationToken ct = default) => Task.CompletedTask;
        public override Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(UserId ownerUserId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<SessionCardProjection>>([]);
        public override Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
            RoomId roomId, FeedCursor? cursor = null, int limit = 20,
            ToolKind? toolKindFilter = null, DateTimeOffset? since = null, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<SessionCardProjection>>([]);
    }

    private sealed class WorkflowSessionRepository(
        Session session,
        RecordingSessionEndAuthority authority,
        RecordingRoomLifecycleLock roomLifecycleLock) : SessionRepositoryStub
    {
        private readonly List<Session> _sessions = [session];

        public bool LockedWhileAuthorityHeld { get; private set; }
        public bool EnumeratedWhileRoomLockHeld { get; private set; }

        public void Add(Session added) => _sessions.Add(added);

        public override Task<IReadOnlyList<SessionId>> GetAllNonEndedByRoomIdsAsync(
            RoomId roomId,
            CancellationToken ct = default)
        {
            EnumeratedWhileRoomLockHeld = roomLifecycleLock.Held;
            return Task.FromResult<IReadOnlyList<SessionId>>(
                _sessions
                    .Where(candidate =>
                        candidate.RoomId == roomId
                        && candidate.Status != SessionStatus.Ended)
                    .Select(candidate => candidate.Id)
                    .ToList());
        }

        public override Task<IReadOnlyList<Session>> GetNonEndedByRoomIdsForUpdateAsync(
            RoomId roomId,
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default)
        {
            LockedWhileAuthorityHeld = authority.LeaseHeld;
            return Task.FromResult<IReadOnlyList<Session>>(
                _sessions
                    .Where(candidate =>
                        candidate.RoomId == roomId
                        && sessionIds.Contains(candidate.Id)
                        && candidate.Status != SessionStatus.Ended)
                    .ToList());
        }

        public override Task UpdateAsync(
            Session updated,
            CancellationToken ct = default) =>
            Task.CompletedTask;
    }

    private sealed class RecordingRoomLifecycleLock : IRoomLifecycleLock
    {
        public bool Held { get; private set; }

        public Task AcquireAsync(
            RoomId roomId,
            CancellationToken ct = default)
        {
            Held = true;
            return Task.CompletedTask;
        }
    }

    private sealed class RecordingSessionEndAuthority : ISessionEndAuthority
    {
        public bool LeaseHeld { get; private set; }
        public IReadOnlyList<SessionId> AcquiredSessionIds { get; private set; } = [];
        public List<CommittedSessionEnd> Projected { get; } = [];
        public bool ProjectedBeforeLeaseRelease { get; private set; }

        public ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default)
        {
            AcquiredSessionIds = [.. sessionIds];
            LeaseHeld = true;
            return ValueTask.FromResult<IAsyncDisposable>(new Lease(this));
        }

        public Task ProjectCommittedAsync(
            IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
            CancellationToken ct = default)
        {
            ProjectedBeforeLeaseRelease = LeaseHeld;
            Projected.AddRange(sessionEnds);
            return Task.CompletedTask;
        }

        public Task RetireEndedIncarnationAsync(
            SessionDiscoveryTarget endedSession,
            DateTimeOffset startedAt,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        private sealed class Lease(RecordingSessionEndAuthority owner)
            : IAsyncDisposable
        {
            public ValueTask DisposeAsync()
            {
                owner.LeaseHeld = false;
                return ValueTask.CompletedTask;
            }
        }
    }

    private sealed class BlockingSessionEndAuthority : ISessionEndAuthority
    {
        private readonly TaskCompletionSource _release =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource AcquireStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public async ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default)
        {
            AcquireStarted.TrySetResult();
            await _release.Task.WaitAsync(ct);
            return new Lease();
        }

        public void Release() => _release.TrySetResult();

        public Task ProjectCommittedAsync(
            IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task RetireEndedIncarnationAsync(
            SessionDiscoveryTarget endedSession,
            DateTimeOffset startedAt,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        private sealed class Lease : IAsyncDisposable
        {
            public ValueTask DisposeAsync() => ValueTask.CompletedTask;
        }
    }

    private sealed class RecordingRoomMemberAuditRepository
        : IRoomMemberAuditRepository
    {
        public Task AddAsync(
            RoomMemberAuditEntry entry,
            CancellationToken ct = default) =>
            Task.CompletedTask;
    }

    private sealed class LocalFakeRoomMemberRepository(RoomMember? member = null) : IRoomMemberRepository
    {
        private RoomMember? _member = member;
        public int AddCalls { get; private set; }

        public Task<bool> IsMemberAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
            => Task.FromResult(_member is not null && _member.RoomId == roomId && _member.UserId == userId && _member.IsActive);
        public Task<IReadOnlyList<RoomId>> GetActiveRoomIdsForUserAsync(
            UserId userId,
            IReadOnlyList<RoomId> roomIds,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<RoomId>>(
                _member is not null
                    && _member.UserId == userId
                    && _member.IsActive
                    && roomIds.Contains(_member.RoomId)
                        ? [_member.RoomId]
                        : []);
        public Task<RoomMember?> GetAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
            => Task.FromResult(_member is not null && _member.RoomId == roomId && _member.UserId == userId ? _member : null);
        public Task AddAsync(RoomMember m, CancellationToken ct = default) { AddCalls++; _member = m; return Task.CompletedTask; }
        public Task<IReadOnlyList<UserId>> GetMemberUserIdsAsync(RoomId roomId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<UserId>>(
                _member is not null && _member.RoomId == roomId && _member.IsActive ? [_member.UserId] : []);

        public Task<IReadOnlyList<RoomMember>> GetActiveByRoomAsync(
            RoomId roomId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<RoomMember>> GetAdmissionProofPageAfterAsync(
            RoomId roomId,
            Guid? afterUserId,
            int limit,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<UserId>> GetRoomPeerUserIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }

    private sealed class LocalFakeRoomRepository(Room? room = null) : IRoomRepository
    {
        public Task<Room?> GetByIdAsync(RoomId id, CancellationToken ct = default)
            => Task.FromResult(room?.Id == id ? room : null);
        public Task AddAsync(Room w, CancellationToken ct = default) => Task.CompletedTask;
        public Task<IReadOnlyList<Room>> GetByMemberPageAsync(
            UserId userId,
            FeedCursor? cursor,
            int limit,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<Room>>(
                room is not null && cursor is null && limit > 0 ? [room] : []);
        public Task<IReadOnlyList<Room>> GetByIdsAsync(
            IReadOnlyCollection<RoomId> ids,
            CancellationToken ct = default)
            => throw new NotSupportedException();
    }

    private sealed class LocalFakeUserRepository(UserId? missingUserId = null) : IUserRepository
    {
        public Task<User?> GetByIdAsync(UserId id, CancellationToken ct = default)
            => Task.FromResult<User?>(id == missingUserId
                ? null
                : User.Create(id, $"{id.Value}@example.com", "handle", "Display"));
        public Task<User?> GetByHandleAsync(string handle, CancellationToken ct = default) => Task.FromResult<User?>(null);
        public Task<IReadOnlyList<string>> GetHandlesByPrefixAsync(string prefix, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<string>>([]);
        public Task<IReadOnlyList<User>> GetByIdsAsync(IReadOnlyList<UserId> ids, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<User>>(
                ids.Select(id => User.Create(id, $"{id.Value}@example.com", "handle", "Display")).ToList());
        public Task AddAsync(User user, CancellationToken ct = default) => Task.CompletedTask;
    }

    private sealed class LocalFakeUnitOfWork : UnitOfWorkStub
    {
        private readonly RecordingTransactionScope _transaction = new();

        public int SaveCalls { get; private set; }
        public int BeginCalls { get; private set; }
        public int CommitCalls => _transaction.CommitCalls;

        public override Task SaveChangesAsync(CancellationToken ct = default) { SaveCalls++; return Task.CompletedTask; }

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default)
        {
            BeginCalls++;
            return Task.FromResult<ITransactionScope>(_transaction);
        }

        private sealed class RecordingTransactionScope : TransactionScopeStub
        {
            public int CommitCalls { get; private set; }

            public override Task CommitAsync(CancellationToken ct = default)
            {
                CommitCalls++;
                return Task.CompletedTask;
            }

            public override ValueTask DisposeAsync() => ValueTask.CompletedTask;
        }
    }

    private sealed class LocalFakeRoomMutationReceiptRepository
        : IRoomMutationReceiptRepository
    {
        private RoomMutationReceipt? _receipt;
        private IReadOnlyList<RoomMutationSessionEffect> _effects = [];

        public Task AcquireAsync(
            UserId actorUserId,
            RoomMutationOperation operation,
            Guid requestId,
            CancellationToken ct = default) => Task.CompletedTask;

        public Task<RoomMutationReceipt?> GetAsync(
            UserId actorUserId,
            RoomMutationOperation operation,
            Guid requestId,
            CancellationToken ct = default) => Task.FromResult(_receipt);

        public Task<RoomMutationReceipt?> FindAsync(
            UserId actorUserId,
            RoomMutationOperation operation,
            Guid requestId,
            CancellationToken ct = default) => Task.FromResult<RoomMutationReceipt?>(
                _receipt?.ActorUserId == actorUserId
                    && _receipt.Operation == operation
                    && _receipt.RequestId == requestId
                    ? _receipt
                    : null);

        public Task AddSessionEffectsAsync(
            IReadOnlyCollection<RoomMutationSessionEffect> effects,
            CancellationToken ct = default)
        {
            _effects = effects.ToList();
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<RoomMutationSessionEffect>> GetSessionEffectsAsync(
            UserId actorUserId,
            Guid requestId,
            CancellationToken ct = default) => Task.FromResult(_effects);

        public Task AddAsync(
            RoomMutationReceipt receipt,
            CancellationToken ct = default)
        {
            _receipt = receipt;
            return Task.CompletedTask;
        }
    }
}
