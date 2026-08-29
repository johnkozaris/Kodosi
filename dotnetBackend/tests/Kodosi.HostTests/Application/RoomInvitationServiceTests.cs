using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class RoomInvitationServiceTests
{
    [Fact]
    public async Task Cancel_Loads_Invitation_After_Lifecycle_Lock()
    {
        var lifecycleLock = new CallbackLifecycleLock();
        var fixture = CreateFixture(lifecycleLock);
        var invitation = await fixture.InviteAsync();
        lifecycleLock.Arm(() => invitation.CancelByInviter(
            invitation.InvitedByUserId,
            DateTimeOffset.UtcNow));

        await Assert.ThrowsAsync<InvalidStateException>(() =>
            fixture.CancelAsync(
                invitation.Id,
                fixture.OwnerId,
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Invite_Holds_Room_Lifecycle_Lock_Through_Commit()
    {
        var roomLock = new RecordingRoomLifecycleLock();
        var fixture = CreateFixture(roomLifecycleLock: roomLock);
        roomLock.Events = fixture.UnitOfWork.Events;

        _ = await fixture.InviteAsync();

        Assert.Equal(
            ["begin", "room-lock", "save", "commit", "dispose"],
            fixture.UnitOfWork.Events);
    }

    [Fact]
    public async Task Accept_Holds_Room_Lifecycle_Lock_Through_Commit()
    {
        var roomLock = new RecordingRoomLifecycleLock();
        var fixture = CreateFixture(roomLifecycleLock: roomLock);
        var invitation = await fixture.InviteAsync();
        fixture.UnitOfWork.Events.Clear();
        roomLock.Events = fixture.UnitOfWork.Events;
        var decision = fixture.BuildDecision(invitation, "accepted");

        _ = await fixture.AcceptAsync(
            invitation.Id,
            fixture.InviteeId,
            decision.Body,
            decision.Signature,
            decision.SignerDeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(
            ["begin", "room-lock", "save", "commit", "dispose"],
            fixture.UnitOfWork.Events);
    }

    [Fact]
    public async Task Invite_Acquires_Owner_And_Invitee_Lifecycle_Locks_In_Id_Order()
    {
        var lifecycleLock = new RecordingLifecycleLock();
        var fixture = CreateFixture(lifecycleLock);

        _ = await fixture.InviteAsync();

        Assert.Equal(
            new[] { fixture.OwnerId, fixture.InviteeId }
                .OrderBy(userId => userId.Value),
            lifecycleLock.Acquired);
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Invite_Stages_Proof_Without_Mutating_Active_Roster()
    {
        var fixture = CreateFixture();
        var baseBody = fixture.Room.RosterBody.ToArray();

        var invitation = await fixture.InviteAsync();

        Assert.Equal(RoomInvitationStatus.Pending, invitation.Status);
        Assert.Equal(1, fixture.Room.RosterGeneration);
        Assert.Equal(baseBody, fixture.Room.RosterBody);
        Assert.False(await fixture.Members.IsMemberAsync(
            fixture.Room.Id,
            fixture.InviteeId,
            TestContext.Current.CancellationToken));
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Accept_With_Valid_Composite_Proof_Promotes_Roster_And_Membership()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        var decision = fixture.BuildDecision(invitation, "accepted");

        var (accepted, transition) = await fixture.AcceptAsync(
            invitation.Id,
            fixture.InviteeId,
            decision.Body,
            decision.Signature,
            decision.SignerDeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(RoomInvitationStatus.Accepted, accepted.Status);
        Assert.Equal(MembershipTransition.Added, transition);
        Assert.Equal(2, fixture.Room.RosterGeneration);
        Assert.Equal(invitation.ProposedRosterBody, fixture.Room.RosterBody);
        Assert.Equal(invitation.Id, fixture.Room.RosterActivationInvitationId);
        Assert.Equal(invitation.ProposalBody, fixture.Room.RosterActivationProposalBody);
        Assert.Equal(decision.Body, fixture.Room.RosterActivationDecisionBody);
        Assert.True(await fixture.Members.IsMemberAsync(
            fixture.Room.Id,
            fixture.InviteeId,
            TestContext.Current.CancellationToken));
        var member = await fixture.Members.GetAsync(
            fixture.Room.Id,
            fixture.InviteeId,
            TestContext.Current.CancellationToken);
        Assert.NotNull(member);
        Assert.Equal(invitation.Id, member.AdmissionInvitationId);
        Assert.Equal(invitation.ProposalBody, member.AdmissionProposalBody);
        Assert.Equal(decision.Body, member.AdmissionDecisionBody);
        var rosterTransition = Assert.Single(fixture.RosterTransitions.Added);
        Assert.Equal(fixture.Room.Id, rosterTransition.RoomId);
        Assert.Equal(2, rosterTransition.Generation);
        Assert.Equal(invitation.Id, rosterTransition.AdmissionInvitationId);
    }

    [Fact]
    public async Task Accept_Retry_Returns_Exact_Accepted_Invitation_Without_Reapplying_Roster()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        var decision = fixture.BuildDecision(invitation, "accepted");
        _ = await fixture.AcceptAsync(
            invitation.Id,
            fixture.InviteeId,
            decision.Body,
            decision.Signature,
            decision.SignerDeviceId,
            TestContext.Current.CancellationToken);

        var (replayed, transition) = await fixture.AcceptAsync(
            invitation.Id,
            fixture.InviteeId,
            decision.Body,
            decision.Signature,
            decision.SignerDeviceId,
            TestContext.Current.CancellationToken);

        Assert.Same(invitation, replayed);
        Assert.Equal(MembershipTransition.NoOp, transition);
        Assert.Equal(2, fixture.Room.RosterGeneration);
        Assert.Single(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Decline_Stores_Decision_Without_Mutating_Roster()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        var decision = fixture.BuildDecision(invitation, "declined");

        var declined = await fixture.DeclineAsync(
            invitation.Id,
            fixture.InviteeId,
            decision.Body,
            decision.Signature,
            decision.SignerDeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(RoomInvitationStatus.Declined, declined.Status);
        Assert.Equal(decision.Body, declined.DecisionBody);
        Assert.Equal(1, fixture.Room.RosterGeneration);
        Assert.False(await fixture.Members.IsMemberAsync(
            fixture.Room.Id,
            fixture.InviteeId,
            TestContext.Current.CancellationToken));
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Cancel_Changes_Only_Invitation_State()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();

        var cancelled = await fixture.CancelAsync(
            invitation.Id,
            fixture.OwnerId,
            TestContext.Current.CancellationToken);

        Assert.Equal(RoomInvitationStatus.Cancelled, cancelled.Status);
        Assert.Null(cancelled.DecisionBody);
        Assert.Equal(1, fixture.Room.RosterGeneration);
        Assert.False(await fixture.Members.IsMemberAsync(
            fixture.Room.Id,
            fixture.InviteeId,
            TestContext.Current.CancellationToken));
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Invite_Rejects_Mismatched_Proposed_Roster_Hash()
    {
        var fixture = CreateFixture();
        var proof = fixture.BuildProposal(proposedRosterBodyHash: new byte[32]);

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            fixture.InviteAsync(proof));

        Assert.Equal(1, fixture.Room.RosterGeneration);
        Assert.Empty(fixture.Invitations.Items);
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Invite_Rejects_Owner_Signature_From_Invitee_Device()
    {
        var fixture = CreateFixture();
        var proof = fixture.BuildProposal(proposalSignerDeviceId: "invitee-device");

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            fixture.InviteAsync(proof));
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Invite_Rejects_Forged_Proposal_Signature()
    {
        var fixture = CreateFixture();
        var proof = fixture.BuildProposal(proposalSignature: [0]);

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            fixture.InviteAsync(proof));
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Invite_Rejects_Forged_Proposed_Roster_Signature()
    {
        var fixture = CreateFixture();
        var proof = fixture.BuildProposal(proposedRosterSignature: [0]);

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            fixture.InviteAsync(proof));
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Invite_Rejects_Expired_Proposal()
    {
        var fixture = CreateFixture();
        var proof = fixture.BuildProposal(
            issuedAt: fixture.Now.AddDays(-2),
            expiresAt: fixture.Now.AddDays(-1));

        await Assert.ThrowsAsync<DomainException>(() =>
            fixture.InviteAsync(proof));
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Accept_Rejects_Base_Generation_Drift()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        fixture.Room.ReplaceRosterForRemoval(2, [9], [9], "owner-device");
        var decision = fixture.BuildDecision(invitation, "accepted");

        await Assert.ThrowsAsync<InvitationLifecycleConflictException>(() =>
            fixture.AcceptAsync(
                invitation.Id,
                fixture.InviteeId,
                decision.Body,
                decision.Signature,
                decision.SignerDeviceId,
                TestContext.Current.CancellationToken));

        Assert.False(await fixture.Members.IsMemberAsync(
            fixture.Room.Id,
            fixture.InviteeId,
            TestContext.Current.CancellationToken));
        Assert.Equal(RoomInvitationStatus.Superseded, invitation.Status);
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Accept_Rejects_Wrong_Device_And_Forged_Decision()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        var wrongDevice = fixture.BuildDecision(
            invitation,
            "accepted",
            signerDeviceId: "owner-device");

        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            fixture.AcceptAsync(
                invitation.Id,
                fixture.InviteeId,
                wrongDevice.Body,
                wrongDevice.Signature,
                wrongDevice.SignerDeviceId,
                TestContext.Current.CancellationToken));

        var forged = fixture.BuildDecision(invitation, "accepted", signature: [0]);
        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            fixture.AcceptAsync(
                invitation.Id,
                fixture.InviteeId,
                forged.Body,
                forged.Signature,
                forged.SignerDeviceId,
                TestContext.Current.CancellationToken));
        Assert.Equal(1, fixture.Room.RosterGeneration);
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Invite_Expires_Stale_Pending_Proposal_And_Allows_Reinvite()
    {
        var fixture = CreateFixture();
        var stale = await fixture.InviteAsync();
        fixture.Time.Advance(TimeSpan.FromDays(8));
        var issuedAt = fixture.Time.GetUtcNow();
        var replacementProof = fixture.BuildProposal(
            issuedAt: issuedAt,
            expiresAt: issuedAt.AddDays(7));

        var replacement = await fixture.InviteAsync(replacementProof);

        Assert.Equal(RoomInvitationStatus.Expired, stale.Status);
        Assert.Equal(RoomInvitationStatus.Pending, replacement.Status);
        Assert.NotEqual(stale.Id, replacement.Id);
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Accept_Supersedes_Competing_Pending_Roster_Proposals()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        var competingInvitee = UserId.New();
        var competing = RoomInvitation.Create(
            Guid.NewGuid(),
            fixture.Room.Id,
            competingInvitee,
            fixture.OwnerId,
            new RoomInvitationProposalProof(
                1,
                2,
                [1],
                [1],
                "owner-device",
                [1],
                [1],
                "owner-device",
                [1],
                fixture.Now,
                fixture.Now.AddDays(7)),
            fixture.Now);
        fixture.Invitations.Items.Add(competing);
        var decision = fixture.BuildDecision(invitation, "accepted");

        _ = await fixture.AcceptAsync(
            invitation.Id,
            fixture.InviteeId,
            decision.Body,
            decision.Signature,
            decision.SignerDeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(RoomInvitationStatus.Superseded, competing.Status);
        Assert.Single(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task Accept_Rejects_Decision_Bound_To_Different_Proposal()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        var decision = fixture.BuildDecision(
            invitation,
            "accepted",
            proposalHash: new byte[32]);

        await Assert.ThrowsAsync<DomainException>(() =>
            fixture.AcceptAsync(
                invitation.Id,
                fixture.InviteeId,
                decision.Body,
                decision.Signature,
                decision.SignerDeviceId,
                TestContext.Current.CancellationToken));
        Assert.Equal(1, fixture.Room.RosterGeneration);
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    [Fact]
    public async Task AcceptIdempotently_Persists_And_Replays_Expired_Conflict()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        fixture.Time.Advance(TimeSpan.FromDays(8));
        var decision = fixture.BuildDecision(
            invitation,
            "accepted",
            issuedAt: fixture.Time.GetUtcNow());
        var requestId = Guid.CreateVersion7();

        var first = await Assert.ThrowsAsync<InvitationLifecycleConflictException>(() =>
            fixture.Service.AcceptIdempotentlyAsync(
                requestId,
                invitation.Id,
                fixture.InviteeId,
                decision.Body,
                decision.Signature,
                decision.SignerDeviceId,
                new RequestAuditContext("127.0.0.1", "xunit"),
                TestContext.Current.CancellationToken));

        Assert.Equal("INVITATION_EXPIRED", first.Code);
        Assert.Equal(RoomInvitationStatus.Expired, invitation.Status);
        Assert.Equal("Expired", Assert.IsType<RoomMutationReceipt>(fixture.Receipts.Receipt).Result);
        Assert.Equal(2, fixture.UnitOfWork.SaveCalls);
        Assert.Equal(2, fixture.UnitOfWork.CommitCalls);

        var duplicate = await Assert.ThrowsAsync<InvitationLifecycleConflictException>(() =>
            fixture.Service.AcceptIdempotentlyAsync(
                requestId,
                invitation.Id,
                fixture.InviteeId,
                decision.Body,
                decision.Signature,
                decision.SignerDeviceId,
                new RequestAuditContext("127.0.0.1", "xunit"),
                TestContext.Current.CancellationToken));

        Assert.Equal("INVITATION_EXPIRED", duplicate.Code);
        Assert.Equal(2, fixture.UnitOfWork.SaveCalls);
        Assert.Equal(3, fixture.UnitOfWork.CommitCalls);
    }

    [Fact]
    public async Task AcceptIdempotently_Persists_And_Replays_Superseded_Conflict()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        fixture.Room.ReplaceRosterForRemoval(2, [3], [4], "owner-device");
        var decision = fixture.BuildDecision(invitation, "accepted");
        var requestId = Guid.CreateVersion7();

        var first = await Assert.ThrowsAsync<InvitationLifecycleConflictException>(() =>
            fixture.Service.AcceptIdempotentlyAsync(
                requestId,
                invitation.Id,
                fixture.InviteeId,
                decision.Body,
                decision.Signature,
                decision.SignerDeviceId,
                new RequestAuditContext("127.0.0.1", "xunit"),
                TestContext.Current.CancellationToken));

        Assert.Equal("INVITATION_SUPERSEDED", first.Code);
        Assert.Equal(RoomInvitationStatus.Superseded, invitation.Status);
        Assert.Equal("Superseded", Assert.IsType<RoomMutationReceipt>(fixture.Receipts.Receipt).Result);

        var duplicate = await Assert.ThrowsAsync<InvitationLifecycleConflictException>(() =>
            fixture.Service.AcceptIdempotentlyAsync(
                requestId,
                invitation.Id,
                fixture.InviteeId,
                decision.Body,
                decision.Signature,
                decision.SignerDeviceId,
                new RequestAuditContext("127.0.0.1", "xunit"),
                TestContext.Current.CancellationToken));

        Assert.Equal("INVITATION_SUPERSEDED", duplicate.Code);
    }

    [Fact]
    public async Task DeclineIdempotently_Persists_And_Replays_Expired_Conflict()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        fixture.Time.Advance(TimeSpan.FromDays(8));
        var decision = fixture.BuildDecision(
            invitation,
            "declined",
            issuedAt: fixture.Time.GetUtcNow());
        var requestId = Guid.CreateVersion7();

        var first = await Assert.ThrowsAsync<InvitationLifecycleConflictException>(() =>
            fixture.Service.DeclineIdempotentlyAsync(
                requestId,
                invitation.Id,
                fixture.InviteeId,
                decision.Body,
                decision.Signature,
                decision.SignerDeviceId,
                TestContext.Current.CancellationToken));

        Assert.Equal("INVITATION_EXPIRED", first.Code);
        Assert.Equal("Expired", Assert.IsType<RoomMutationReceipt>(fixture.Receipts.Receipt).Result);

        var duplicate = await Assert.ThrowsAsync<InvitationLifecycleConflictException>(() =>
            fixture.Service.DeclineIdempotentlyAsync(
                requestId,
                invitation.Id,
                fixture.InviteeId,
                decision.Body,
                decision.Signature,
                decision.SignerDeviceId,
                TestContext.Current.CancellationToken));

        Assert.Equal("INVITATION_EXPIRED", duplicate.Code);
    }

    [Fact]
    public async Task Accept_Rejects_Expired_Proposal()
    {
        var fixture = CreateFixture();
        var invitation = await fixture.InviteAsync();
        fixture.Time.Advance(TimeSpan.FromDays(8));
        var decision = fixture.BuildDecision(
            invitation,
            "accepted",
            issuedAt: fixture.Time.GetUtcNow());

        await Assert.ThrowsAsync<InvitationLifecycleConflictException>(() =>
            fixture.AcceptAsync(
                invitation.Id,
                fixture.InviteeId,
                decision.Body,
                decision.Signature,
                decision.SignerDeviceId,
                TestContext.Current.CancellationToken));
        Assert.Equal(1, fixture.Room.RosterGeneration);
        Assert.Equal(RoomInvitationStatus.Expired, invitation.Status);
        Assert.Empty(fixture.RosterTransitions.Added);
    }

    private static Fixture CreateFixture(
        IUserLifecycleLock? lifecycleLock = null,
        IRoomLifecycleLock? roomLifecycleLock = null)
    {
        var now = new DateTimeOffset(2026, 7, 12, 7, 30, 0, TimeSpan.Zero);
        var time = new TestTimeProvider(now);
        var ownerId = UserId.New();
        var inviteeId = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()),
            ownerId,
            "Room",
            "room",
            1,
            [1],
            [2],
            "owner-device");
        var members = new FakeRoomMemberRepository((room.Id, ownerId));
        var invitations = new FakeInvitationRepository();
        var owner = User.Create(
            ownerId,
            "owner@example.test",
            "owner",
            "Owner");
        var invitee = User.Create(
            inviteeId,
            "invitee@example.test",
            "invitee",
            "Invitee");
        var ownerDevice = CreateDevice(ownerId, "owner-device", now);
        var inviteeDevice = CreateDevice(inviteeId, "invitee-device", now);
        var deviceRepository = new FakeUserDeviceRepository(ownerDevice, inviteeDevice);
        var deviceLists = new FakeUserDeviceListRepository(
            CreateDeviceList(ownerId, "owner-device", now),
            CreateDeviceList(inviteeId, "invitee-device", now));
        var signatureVerifier = new SignatureByteVerifier();
        var rosterVerifier = new RoomRosterVerifier(
            deviceRepository,
            deviceLists,
            signatureVerifier,
            time);
        var proofVerifier = new RoomInvitationProofVerifier(
            deviceRepository,
            deviceLists,
            signatureVerifier,
            time);
        var rosterTransitions = new RecordingRoomRosterTransitionRepository();
        var receipts = new FakeRoomMutationReceiptRepository();
        var unitOfWork = new FakeUnitOfWork();
        var service = new RoomInvitationService(
            new FakeRoomRepository(room),
            members,
            invitations,
            new FakeUserRepository(owner, invitee),
            rosterVerifier,
            rosterTransitions,
            proofVerifier,
            time,
            lifecycleLock ?? new RecordingLifecycleLock(),
            roomLifecycleLock ?? new FakeRoomLifecycleLock(),
            receipts,
            unitOfWork,
            new FakeRoomMemberAuditRepository());
        return new Fixture(
            service,
            room,
            ownerId,
            inviteeId,
            members,
            invitations,
            rosterTransitions,
            receipts,
            unitOfWork,
            time,
            now);
    }

    private static UserDevice CreateDevice(
        UserId userId,
        string deviceId,
        DateTimeOffset now)
    {
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: deviceId,
            signerDeviceId: deviceId,
            issuedAt: now.AddMinutes(-1),
            expiresAt: null);
        return device;
    }

    private static UserDeviceList CreateDeviceList(
        UserId userId,
        string deviceId,
        DateTimeOffset now)
        => TestDeviceList.Create(
            userId,
            1,
            JsonSerializer.Serialize(new[]
            {
                new { deviceId, signerDeviceId = deviceId },
            }),
            deviceId,
            [2],
            now.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);

    private sealed record ProposalInput(
        Guid InvitationId,
        byte[] ProposalBody,
        byte[] ProposalSignature,
        string ProposalSignerDeviceId,
        byte[] ProposedRosterBody,
        byte[] ProposedRosterSignature,
        string ProposedRosterSignerDeviceId);

    private sealed record DecisionInput(
        byte[] Body,
        byte[] Signature,
        string SignerDeviceId);

    private sealed record Fixture(
        RoomInvitationService Service,
        Room Room,
        UserId OwnerId,
        UserId InviteeId,
        FakeRoomMemberRepository Members,
        FakeInvitationRepository Invitations,
        RecordingRoomRosterTransitionRepository RosterTransitions,
        FakeRoomMutationReceiptRepository Receipts,
        FakeUnitOfWork UnitOfWork,
        TestTimeProvider Time,
        DateTimeOffset Now)
    {
        public Task<RoomInvitation> InviteAsync(ProposalInput? input = null)
        {
            input ??= BuildProposal();
            return Service.InviteAsync(
                input.InvitationId,
                Room.Id,
                OwnerId,
                InviteeId,
                input.ProposalBody,
                input.ProposalSignature,
                input.ProposalSignerDeviceId,
                2,
                input.ProposedRosterBody,
                input.ProposedRosterSignature,
                input.ProposedRosterSignerDeviceId,
                TestContext.Current.CancellationToken);
        }

        public async Task<(RoomInvitation Invitation, MembershipTransition Transition)> AcceptAsync(
            Guid invitationId,
            UserId actingUserId,
            byte[] decisionBody,
            byte[] decisionSignature,
            string decisionSignerDeviceId,
            CancellationToken ct)
        {
            var result = await Service.AcceptIdempotentlyAsync(
                Guid.CreateVersion7(),
                invitationId,
                actingUserId,
                decisionBody,
                decisionSignature,
                decisionSignerDeviceId,
                new RequestAuditContext("127.0.0.1", "xunit"),
                ct);
            return (result.Invitation, result.MembershipTransition);
        }

        public async Task<RoomInvitation> DeclineAsync(
            Guid invitationId,
            UserId actingUserId,
            byte[] decisionBody,
            byte[] decisionSignature,
            string decisionSignerDeviceId,
            CancellationToken ct)
        {
            var result = await Service.DeclineIdempotentlyAsync(
                Guid.CreateVersion7(),
                invitationId,
                actingUserId,
                decisionBody,
                decisionSignature,
                decisionSignerDeviceId,
                ct);
            return result.Invitation;
        }

        public async Task<RoomInvitation> CancelAsync(
            Guid invitationId,
            UserId actingUserId,
            CancellationToken ct)
        {
            var result = await Service.CancelIdempotentlyAsync(
                Guid.CreateVersion7(),
                invitationId,
                actingUserId,
                ct);
            return result.Invitation;
        }

        public ProposalInput BuildProposal(
            byte[]? proposedRosterBodyHash = null,
            string proposalSignerDeviceId = "owner-device",
            byte[]? proposalSignature = null,
            byte[]? proposedRosterSignature = null,
            DateTimeOffset? issuedAt = null,
            DateTimeOffset? expiresAt = null)
        {
            var invitationId = Guid.NewGuid();
            var proposedRosterBody = RoomRosterTestData.Build(
                Room,
                2,
                [OwnerId, InviteeId],
                issuedAt: Now);
            var issued = issuedAt ?? Now;
            var expires = expiresAt ?? issued.AddDays(7);
            var proposalBody = JsonSerializer.SerializeToUtf8Bytes(new
            {
                version = 1,
                invitationId,
                roomId = Room.Id.Value,
                ownerUserId = OwnerId.Value,
                inviteeUserId = InviteeId.Value,
                baseRosterGeneration = 1,
                proposedRosterGeneration = 2,
                proposedRosterBodyHash = Convert.ToBase64String(
                    proposedRosterBodyHash ?? SHA256.HashData(proposedRosterBody)),
                expiresAtMs = expires.ToUnixTimeMilliseconds(),
                signerDeviceId = proposalSignerDeviceId,
                issuedAtMs = issued.ToUnixTimeMilliseconds(),
            });
            return new ProposalInput(
                invitationId,
                proposalBody,
                proposalSignature ?? [1],
                proposalSignerDeviceId,
                proposedRosterBody,
                proposedRosterSignature ?? [1],
                "owner-device");
        }

        public DecisionInput BuildDecision(
            RoomInvitation invitation,
            string decision,
            string signerDeviceId = "invitee-device",
            byte[]? signature = null,
            DateTimeOffset? issuedAt = null,
            byte[]? proposalHash = null)
        {
            var body = JsonSerializer.SerializeToUtf8Bytes(new
            {
                version = 1,
                invitationId = invitation.Id,
                proposalHash = Convert.ToBase64String(
                    proposalHash ?? invitation.ProposalHash),
                roomId = invitation.RoomId.Value,
                inviteeUserId = invitation.InviteeUserId.Value,
                decision,
                signerDeviceId,
                issuedAtMs = (issuedAt ?? Time.GetUtcNow()).ToUnixTimeMilliseconds(),
            });
            return new DecisionInput(body, signature ?? [1], signerDeviceId);
        }
    }

    private sealed class SignatureByteVerifier : IPopSignatureVerifier
    {
        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature)
            => !signature.IsEmpty && signature[0] == 1;
    }

    private sealed class CallbackLifecycleLock : IUserLifecycleLock
    {
        private Action? _callback;

        public void Arm(Action callback) => _callback = callback;

        public Task AcquireAsync(UserId userId, CancellationToken ct = default)
        {
            var callback = _callback;
            _callback = null;
            callback?.Invoke();
            return Task.CompletedTask;
        }
    }

    private sealed class RecordingRoomLifecycleLock : IRoomLifecycleLock
    {
        public List<string>? Events { get; set; }

        public Task AcquireAsync(RoomId roomId, CancellationToken ct = default)
        {
            Events?.Add("room-lock");
            return Task.CompletedTask;
        }
    }

    private sealed class RecordingLifecycleLock : IUserLifecycleLock
    {
        public List<UserId> Acquired { get; } = [];

        public Task AcquireAsync(UserId userId, CancellationToken ct = default)
        {
            Acquired.Add(userId);
            return Task.CompletedTask;
        }
    }

    private sealed class FakeRoomMutationReceiptRepository : IRoomMutationReceiptRepository
    {
        public RoomMutationReceipt? Receipt { get; private set; }

        public Task AcquireAsync(
            UserId actorUserId,
            RoomMutationOperation operation,
            Guid requestId,
            CancellationToken ct = default) => Task.CompletedTask;

        public Task<RoomMutationReceipt?> GetAsync(
            UserId actorUserId,
            RoomMutationOperation operation,
            Guid requestId,
            CancellationToken ct = default) => Task.FromResult(
                Receipt?.ActorUserId == actorUserId
                    && Receipt.Operation == operation
                    && Receipt.RequestId == requestId
                        ? Receipt
                        : null);

        public Task<RoomMutationReceipt?> FindAsync(
            UserId actorUserId,
            RoomMutationOperation operation,
            Guid requestId,
            CancellationToken ct = default) =>
            GetAsync(actorUserId, operation, requestId, ct);

        public Task AddAsync(
            RoomMutationReceipt receipt,
            CancellationToken ct = default)
        {
            Receipt = receipt;
            return Task.CompletedTask;
        }

        public Task AddSessionEffectsAsync(
            IReadOnlyCollection<RoomMutationSessionEffect> effects,
            CancellationToken ct = default) => Task.CompletedTask;

        public Task<IReadOnlyList<RoomMutationSessionEffect>> GetSessionEffectsAsync(
            UserId actorUserId,
            Guid requestId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<RoomMutationSessionEffect>>([]);
    }

    private sealed class FakeUnitOfWork : UnitOfWorkStub
    {
        public List<string> Events { get; } = [];
        public int SaveCalls { get; private set; }
        public int CommitCalls { get; private set; }

        public override Task SaveChangesAsync(CancellationToken ct = default)
        {
            SaveCalls++;
            Events.Add("save");
            return Task.CompletedTask;
        }

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default)
        {
            Events.Add("begin");
            return Task.FromResult<ITransactionScope>(new Transaction(this));
        }

        private sealed class Transaction(FakeUnitOfWork owner) : TransactionScopeStub
        {
            public override Task CommitAsync(CancellationToken ct = default)
            {
                owner.CommitCalls++;
                owner.Events.Add("commit");
                return Task.CompletedTask;
            }

            public override ValueTask DisposeAsync()
            {
                owner.Events.Add("dispose");
                return ValueTask.CompletedTask;
            }
        }
    }

    private sealed class FakeRoomMemberAuditRepository : IRoomMemberAuditRepository
    {
        public Task AddAsync(
            RoomMemberAuditEntry entry,
            CancellationToken ct = default) => Task.CompletedTask;
    }

    private sealed class FakeInvitationRepository : IRoomInvitationRepository
    {
        public List<RoomInvitation> Items { get; } = [];

        public Task<RoomInvitation?> GetByIdAsync(
            Guid id,
            CancellationToken ct = default)
            => Task.FromResult(Items.FirstOrDefault(invitation => invitation.Id == id));

        public Task<IReadOnlyList<RoomInvitation>> GetByIdsAsync(
            IReadOnlyCollection<Guid> ids,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<RoomInvitation>>(
                Items.Where(invitation => ids.Contains(invitation.Id)).ToList());

        public Task<RoomInvitationLockUsers?> GetLockUsersAsync(
            Guid id,
            CancellationToken ct = default)
        {
            var invitation = Items.FirstOrDefault(candidate => candidate.Id == id);
            return Task.FromResult(
                invitation is null
                    ? (RoomInvitationLockUsers?)null
                    : new RoomInvitationLockUsers(
                        invitation.RoomId,
                        invitation.InvitedByUserId,
                        invitation.InviteeUserId));
        }

        public Task<RoomInvitation?> GetPendingAsync(
            RoomId roomId,
            UserId inviteeUserId,
            CancellationToken ct = default)
            => Task.FromResult(Items.FirstOrDefault(invitation =>
                invitation.RoomId == roomId
                && invitation.InviteeUserId == inviteeUserId
                && invitation.Status == RoomInvitationStatus.Pending));

        public Task<IReadOnlyList<RoomInvitation>> GetIncomingAsync(
            UserId userId,
            int limit,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<RoomInvitation>>(Items
                .Where(invitation =>
                    invitation.InviteeUserId == userId
                    && invitation.Status == RoomInvitationStatus.Pending)
                .Take(Math.Clamp(limit, 1, 100))
                .ToList());

        public Task<IReadOnlyList<RoomInvitation>> GetOutgoingAsync(
            UserId userId,
            int limit,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<RoomInvitation>>(Items
                .Where(invitation =>
                    invitation.InvitedByUserId == userId
                    && invitation.Status == RoomInvitationStatus.Pending)
                .Take(Math.Clamp(limit, 1, 100))
                .ToList());

        public Task<IReadOnlyList<RoomInvitation>> GetPendingByRoomAsync(
            RoomId roomId,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<RoomInvitation>>(Items
                .Where(invitation =>
                    invitation.RoomId == roomId
                    && invitation.Status == RoomInvitationStatus.Pending)
                .ToList());

        public Task<IReadOnlyList<RoomInvitation>> GetExpiredPendingAsync(
            DateTimeOffset now,
            int limit,
            CancellationToken ct = default)
            => throw new NotSupportedException();

        public Task<IReadOnlyList<RoomInvitation>> GetGenerationDriftPendingAsync(
            int limit,
            CancellationToken ct = default)
            => throw new NotSupportedException();

        public Task AddAsync(
            RoomInvitation invitation,
            CancellationToken ct = default)
        {
            Items.Add(invitation);
            return Task.CompletedTask;
        }
    }
}
