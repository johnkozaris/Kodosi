using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class RoomInvitationService(
    IRoomRepository rooms,
    IRoomMemberRepository roomMembers,
    IRoomInvitationRepository invitations,
    IUserRepository users,
    RoomRosterVerifier rosterVerifier,
    IRoomRosterTransitionRepository rosterTransitions,
    RoomInvitationProofVerifier invitationProofVerifier,
    TimeProvider timeProvider,
    IUserLifecycleLock userLifecycleLock,
    IRoomLifecycleLock roomLifecycleLock,
    IRoomMutationReceiptRepository mutationReceipts,
    IUnitOfWork unitOfWork,
    IRoomMemberAuditRepository roomMemberAudit)
{
    private const int InvitationListHardLimit = 100;
    private readonly IRoomRepository _rooms = rooms;
    private readonly IRoomMemberRepository _roomMembers = roomMembers;
    private readonly IRoomInvitationRepository _invitations = invitations;
    private readonly IUserRepository _users = users;
    private readonly RoomRosterVerifier _rosterVerifier = rosterVerifier;
    private readonly IRoomRosterTransitionRepository _rosterTransitions = rosterTransitions;
    private readonly RoomInvitationProofVerifier _invitationProofVerifier =
        invitationProofVerifier;
    private readonly TimeProvider _timeProvider = timeProvider;
    private readonly IUserLifecycleLock _userLifecycleLock = userLifecycleLock;
    private readonly IRoomLifecycleLock _roomLifecycleLock = roomLifecycleLock;
    private readonly IRoomMutationReceiptRepository _mutationReceipts = mutationReceipts;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;
    private readonly IRoomMemberAuditRepository _roomMemberAudit = roomMemberAudit;

    public async Task<RoomInvitation> InviteAsync(
        Guid invitationId,
        RoomId roomId,
        UserId inviterUserId,
        UserId inviteeUserId,
        byte[] proposalBody,
        byte[] proposalSignature,
        string proposalSignerDeviceId,
        long proposedRosterGeneration,
        byte[] proposedRosterBody,
        byte[] proposedRosterSignature,
        string proposedRosterSignerDeviceId,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await AcquireLifecycleLocksAsync([inviterUserId, inviteeUserId], ct);
        await _roomLifecycleLock.AcquireAsync(roomId, ct);
        var invitation = await InviteCoreAsync(
            invitationId,
            roomId,
            inviterUserId,
            inviteeUserId,
            proposalBody,
            proposalSignature,
            proposalSignerDeviceId,
            proposedRosterGeneration,
            proposedRosterBody,
            proposedRosterSignature,
            proposedRosterSignerDeviceId,
            ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return invitation;
    }

    private async Task<RoomInvitation> InviteCoreAsync(
        Guid invitationId,
        RoomId roomId,
        UserId inviterUserId,
        UserId inviteeUserId,
        byte[] proposalBody,
        byte[] proposalSignature,
        string proposalSignerDeviceId,
        long proposedRosterGeneration,
        byte[] proposedRosterBody,
        byte[] proposedRosterSignature,
        string proposedRosterSignerDeviceId,
        CancellationToken ct)
    {
        var room = await _rooms.GetByIdAsync(roomId, ct)
            ?? throw new NotFoundException(nameof(Room), roomId);
        if (!room.IsOwner(inviterUserId))
        {
            throw new PolicyViolationException(
                "Only the room owner may invite new members.");
        }
        if (await _roomMembers.IsMemberAsync(roomId, inviteeUserId, ct))
        {
            throw new RoomMemberAlreadyExistsException();
        }

        _ = await _users.GetByIdAsync(inviteeUserId, ct)
            ?? throw new NotFoundException(nameof(User), inviteeUserId);

        var existingById = await _invitations.GetByIdAsync(invitationId, ct);
        if (existingById is not null)
        {
            if (existingById.MatchesCreation(
                    roomId,
                    inviteeUserId,
                    inviterUserId,
                    proposalBody,
                    proposalSignature,
                    proposalSignerDeviceId,
                    proposedRosterGeneration,
                    proposedRosterBody,
                    proposedRosterSignature,
                    proposedRosterSignerDeviceId))
            {
                return existingById;
            }
            throw new ConflictException("Invitation ID is already in use.");
        }

        var now = _timeProvider.GetUtcNow();
        var existing = await _invitations.GetPendingAsync(roomId, inviteeUserId, ct);
        if (existing is not null && existing.IsExpired(now))
        {
            existing.Expire(now);
            existing = null;
        }
        else if (existing is not null
            && existing.BaseRosterGeneration != room.RosterGeneration)
        {
            existing.Supersede(now);
            existing = null;
        }
        if (existing is not null)
        {
            if (existing.Id == invitationId)
            {
                return existing;
            }
            throw new ConflictException(
                "A different pending invitation already exists for this room member.");
        }

        var verifiedProposal = await _invitationProofVerifier.VerifyProposalAsync(
            room,
            invitationId,
            inviteeUserId,
            proposalBody,
            proposalSignature,
            proposalSignerDeviceId,
            proposedRosterBody,
            ct);
        if (proposedRosterGeneration != verifiedProposal.ProposedRosterGeneration)
        {
            throw new DomainException(
                "Proposed roster generation does not match the invitation proposal.");
        }

        var expectedMembers = (await _roomMembers.GetMemberUserIdsAsync(roomId, ct))
            .ToHashSet();
        expectedMembers.Add(room.OwnerUserId);
        expectedMembers.Add(inviteeUserId);
        await _rosterVerifier.VerifyAsync(
            roomId,
            inviterUserId,
            proposedRosterGeneration,
            proposedRosterBody,
            proposedRosterSignature,
            proposedRosterSignerDeviceId,
            expectedMembers,
            ct);

        var invitation = RoomInvitation.Create(
            invitationId,
            roomId,
            inviteeUserId,
            inviterUserId,
            new RoomInvitationProposalProof(
                verifiedProposal.BaseRosterGeneration,
                verifiedProposal.ProposedRosterGeneration,
                proposedRosterBody,
                proposedRosterSignature,
                proposedRosterSignerDeviceId,
                proposalBody,
                proposalSignature,
                proposalSignerDeviceId,
                verifiedProposal.ProposalHash,
                verifiedProposal.IssuedAt,
                verifiedProposal.ExpiresAt),
            now);
        await _invitations.AddAsync(invitation, ct);
        return invitation;
    }

    public async Task<RoomInvitationMutationResult> AcceptIdempotentlyAsync(
        Guid requestId,
        Guid invitationId,
        UserId actingUserId,
        byte[] decisionBody,
        byte[] decisionSignature,
        string decisionSignerDeviceId,
        RequestAuditContext auditContext,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _mutationReceipts.AcquireAsync(
            actingUserId,
            RoomMutationOperation.AcceptInvitation,
            requestId,
            ct);
        var invitation = await LoadLockedInvitationAsync(invitationId, ct);
        RoomInvitationAuthorization.EnsureActor(invitation, actingUserId, inviteeAction: true);
        var fingerprint = RoomMutationTargetFingerprint.AcceptInvitation(
            invitation.RoomId,
            invitation.Id,
            invitation.BaseRosterGeneration,
            invitation.ProposedRosterGeneration,
            decisionBody,
            decisionSignature,
            decisionSignerDeviceId);
        var existingReceipt = await _mutationReceipts.GetAsync(
            actingUserId,
            RoomMutationOperation.AcceptInvitation,
            requestId,
            ct);
        if (existingReceipt is not null)
        {
            RoomMutationReceiptPolicy.EnsureOperation(
                existingReceipt,
                RoomMutationOperation.AcceptInvitation);
            var duplicate = RoomMutationReceiptPolicy.ResolveDuplicate(
                existingReceipt,
                fingerprint);
            await transaction.CommitAsync(ct);
            ThrowIfInvitationLifecycleConflict(duplicate.Result);
            return new RoomInvitationMutationResult(duplicate, invitation, MembershipTransition.NoOp);
        }

        MembershipTransition membershipTransition;
        try
        {
            membershipTransition = await AcceptCoreAsync(
                invitation,
                actingUserId,
                decisionBody,
                decisionSignature,
                decisionSignerDeviceId,
                ct);
        }
        catch (InvitationLifecycleConflictException)
        {
            await PersistInvitationReceiptAsync(
                _mutationReceipts,
                actingUserId,
                RoomMutationOperation.AcceptInvitation,
                requestId,
                fingerprint,
                invitation,
                invitation.ProposedRosterGeneration,
                ct);
            await _unitOfWork.SaveChangesAsync(ct);
            await transaction.CommitAsync(ct);
            throw;
        }
        if (membershipTransition != MembershipTransition.NoOp)
        {
            await _roomMemberAudit.AddAsync(
                RoomMemberAuditEntry.Create(
                    invitation.RoomId,
                    invitation.InvitedByUserId,
                    invitation.InviteeUserId,
                    RoomMemberAuditAction.Added,
                    auditContext.ClientIp,
                    auditContext.UserAgent),
                ct);
        }
        var receipt = RoomMutationReceiptPolicy.Create(
            actingUserId,
            RoomMutationOperation.AcceptInvitation,
            requestId,
            fingerprint,
            invitation.RoomId,
            invitation.Id,
            invitation.Status.ToString(),
            invitation.ProposedRosterGeneration,
            assigneeSessionId: null,
            assigneeSessionIncarnationId: null,
            _timeProvider.GetUtcNow());
        await _mutationReceipts.AddAsync(receipt, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return new RoomInvitationMutationResult(
            new RoomMutationReceiptResult(
                false,
                receipt.RoomId,
                receipt.EntityId,
                receipt.Result,
                receipt.Revision,
                null,
                null),
            invitation,
            membershipTransition);
    }

    private async Task<MembershipTransition> AcceptCoreAsync(
        RoomInvitation invitation,
        UserId actingUserId,
        byte[] decisionBody,
        byte[] decisionSignature,
        string decisionSignerDeviceId,
        CancellationToken ct)
    {
        if (invitation.Status == RoomInvitationStatus.Accepted
            && invitation.InviteeUserId == actingUserId)
        {
            return MembershipTransition.NoOp;
        }
        if (invitation.Status != RoomInvitationStatus.Pending)
        {
            throw new InvalidStateException(
                $"Invitation is already {invitation.Status}; no further transition allowed.");
        }
        var room = await _rooms.GetByIdAsync(invitation.RoomId, ct)
            ?? throw new NotFoundException(nameof(Room), invitation.RoomId);
        var now = _timeProvider.GetUtcNow();
        if (invitation.IsExpired(now))
        {
            invitation.Expire(now);
            throw new InvitationLifecycleConflictException(RoomInvitationStatus.Expired);
        }
        if (room.RosterGeneration != invitation.BaseRosterGeneration)
        {
            invitation.Supersede(now);
            throw new InvitationLifecycleConflictException(RoomInvitationStatus.Superseded);
        }

        _ = await _invitationProofVerifier.VerifyStoredProposalAsync(
            invitation,
            room,
            ct);
        var expectedMembers = (await _roomMembers.GetMemberUserIdsAsync(
                invitation.RoomId,
                ct))
            .ToHashSet();
        expectedMembers.Add(room.OwnerUserId);
        expectedMembers.Add(invitation.InviteeUserId);
        await _rosterVerifier.VerifyAsync(
            invitation.RoomId,
            invitation.InvitedByUserId,
            invitation.ProposedRosterGeneration,
            invitation.ProposedRosterBody,
            invitation.ProposedRosterSignature,
            invitation.ProposedRosterSignerDeviceId,
            expectedMembers,
            ct);
        var decision = await _invitationProofVerifier.VerifyDecisionAsync(
            invitation,
            actingUserId,
            "accepted",
            decisionBody,
            decisionSignature,
            decisionSignerDeviceId,
            ct);
        invitation.Accept(
            actingUserId,
            new RoomInvitationDecisionProof(
                decisionBody,
                decisionSignature,
                decisionSignerDeviceId,
                decision.IssuedAt),
            now);
        room.ActivateInvitation(invitation);
        await _rosterTransitions.AddAsync(
            RoomRosterTransition.Create(room, invitation.Id),
            ct);

        var competing = await _invitations.GetPendingByRoomAsync(invitation.RoomId, ct);
        foreach (var pending in competing)
        {
            if (pending.Id != invitation.Id)
            {
                pending.Supersede(now);
            }
        }

        var existing = await _roomMembers.GetAsync(invitation.RoomId, actingUserId, ct);
        if (existing is null)
        {
            await _roomMembers.AddAsync(
                RoomMember.CreateFromInvitation(invitation),
                ct);
            return MembershipTransition.Added;
        }
        var wasActive = existing.IsActive;
        existing.ApplyAcceptedInvitation(invitation);
        if (!wasActive)
        {
            return MembershipTransition.Restored;
        }
        return MembershipTransition.NoOp;
    }

    public async Task<RoomInvitationMutationResult> DeclineIdempotentlyAsync(
        Guid requestId,
        Guid invitationId,
        UserId actingUserId,
        byte[] decisionBody,
        byte[] decisionSignature,
        string decisionSignerDeviceId,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _mutationReceipts.AcquireAsync(
            actingUserId,
            RoomMutationOperation.DeclineInvitation,
            requestId,
            ct);
        var invitation = await LoadLockedInvitationAsync(invitationId, ct);
        RoomInvitationAuthorization.EnsureActor(invitation, actingUserId, inviteeAction: true);
        var fingerprint = RoomMutationTargetFingerprint.DeclineInvitation(
            invitation.RoomId,
            invitation.Id,
            invitation.BaseRosterGeneration,
            decisionBody,
            decisionSignature,
            decisionSignerDeviceId);
        var existingReceipt = await _mutationReceipts.GetAsync(
            actingUserId,
            RoomMutationOperation.DeclineInvitation,
            requestId,
            ct);
        if (existingReceipt is not null)
        {
            RoomMutationReceiptPolicy.EnsureOperation(
                existingReceipt,
                RoomMutationOperation.DeclineInvitation);
            var duplicate = RoomMutationReceiptPolicy.ResolveDuplicate(
                existingReceipt,
                fingerprint);
            await transaction.CommitAsync(ct);
            ThrowIfInvitationLifecycleConflict(duplicate.Result);
            return new RoomInvitationMutationResult(duplicate, invitation, MembershipTransition.NoOp);
        }

        try
        {
            await DeclineCoreAsync(
                invitation,
                actingUserId,
                decisionBody,
                decisionSignature,
                decisionSignerDeviceId,
                ct);
        }
        catch (InvitationLifecycleConflictException)
        {
            await PersistInvitationReceiptAsync(
                _mutationReceipts,
                actingUserId,
                RoomMutationOperation.DeclineInvitation,
                requestId,
                fingerprint,
                invitation,
                invitation.BaseRosterGeneration,
                ct);
            await _unitOfWork.SaveChangesAsync(ct);
            await transaction.CommitAsync(ct);
            throw;
        }
        var receipt = RoomMutationReceiptPolicy.Create(
            actingUserId,
            RoomMutationOperation.DeclineInvitation,
            requestId,
            fingerprint,
            invitation.RoomId,
            invitation.Id,
            invitation.Status.ToString(),
            invitation.BaseRosterGeneration,
            assigneeSessionId: null,
            assigneeSessionIncarnationId: null,
            _timeProvider.GetUtcNow());
        await _mutationReceipts.AddAsync(receipt, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return new RoomInvitationMutationResult(
            new RoomMutationReceiptResult(
                false,
                receipt.RoomId,
                receipt.EntityId,
                receipt.Result,
                receipt.Revision,
                null,
                null),
            invitation,
            MembershipTransition.NoOp);
    }

    private async Task DeclineCoreAsync(
        RoomInvitation invitation,
        UserId actingUserId,
        byte[] decisionBody,
        byte[] decisionSignature,
        string decisionSignerDeviceId,
        CancellationToken ct)
    {
        var now = _timeProvider.GetUtcNow();
        if (invitation.IsExpired(now))
        {
            invitation.Expire(now);
            throw new InvitationLifecycleConflictException(RoomInvitationStatus.Expired);
        }
        var decision = await _invitationProofVerifier.VerifyDecisionAsync(
            invitation,
            actingUserId,
            "declined",
            decisionBody,
            decisionSignature,
            decisionSignerDeviceId,
            ct);
        invitation.Decline(
            actingUserId,
            new RoomInvitationDecisionProof(
                decisionBody,
                decisionSignature,
                decisionSignerDeviceId,
                decision.IssuedAt),
            now);
    }

    public async Task<RoomInvitationMutationResult> CancelIdempotentlyAsync(
        Guid requestId,
        Guid invitationId,
        UserId actingUserId,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _mutationReceipts.AcquireAsync(
            actingUserId,
            RoomMutationOperation.CancelInvitation,
            requestId,
            ct);
        var invitation = await LoadLockedInvitationAsync(invitationId, ct);
        RoomInvitationAuthorization.EnsureActor(invitation, actingUserId, inviteeAction: false);
        var fingerprint = RoomMutationTargetFingerprint.CancelInvitation(
            invitation.RoomId,
            invitation.Id,
            invitation.BaseRosterGeneration);
        var existingReceipt = await _mutationReceipts.GetAsync(
            actingUserId,
            RoomMutationOperation.CancelInvitation,
            requestId,
            ct);
        if (existingReceipt is not null)
        {
            RoomMutationReceiptPolicy.EnsureOperation(
                existingReceipt,
                RoomMutationOperation.CancelInvitation);
            var duplicate = RoomMutationReceiptPolicy.ResolveDuplicate(
                existingReceipt,
                fingerprint);
            await transaction.CommitAsync(ct);
            return new RoomInvitationMutationResult(duplicate, invitation, MembershipTransition.NoOp);
        }

        CancelCore(invitation, actingUserId);
        var receipt = RoomMutationReceiptPolicy.Create(
            actingUserId,
            RoomMutationOperation.CancelInvitation,
            requestId,
            fingerprint,
            invitation.RoomId,
            invitation.Id,
            invitation.Status.ToString(),
            invitation.BaseRosterGeneration,
            assigneeSessionId: null,
            assigneeSessionIncarnationId: null,
            _timeProvider.GetUtcNow());
        await _mutationReceipts.AddAsync(receipt, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return new RoomInvitationMutationResult(
            new RoomMutationReceiptResult(
                false,
                receipt.RoomId,
                receipt.EntityId,
                receipt.Result,
                receipt.Revision,
                null,
                null),
            invitation,
            MembershipTransition.NoOp);
    }

    private void CancelCore(RoomInvitation invitation, UserId actingUserId) =>
        invitation.CancelByInviter(actingUserId, _timeProvider.GetUtcNow());

    private async Task<RoomInvitation> LoadLockedInvitationAsync(
        Guid invitationId,
        CancellationToken ct)
    {
        var lockUsers = await _invitations.GetLockUsersAsync(invitationId, ct)
            ?? throw new NotFoundException("RoomInvitation", invitationId);
        await AcquireLifecycleLocksAsync(
            [lockUsers.OwnerUserId, lockUsers.InviteeUserId],
            ct);
        await _roomLifecycleLock.AcquireAsync(lockUsers.RoomId, ct);
        return await _invitations.GetByIdAsync(invitationId, ct)
            ?? throw new NotFoundException("RoomInvitation", invitationId);
    }

    private async Task PersistInvitationReceiptAsync(
        IRoomMutationReceiptRepository _mutationReceipts,
        UserId actingUserId,
        RoomMutationOperation operation,
        Guid requestId,
        RoomMutationTargetFingerprint fingerprint,
        RoomInvitation invitation,
        long revision,
        CancellationToken ct)
    {
        var receipt = RoomMutationReceiptPolicy.Create(
            actingUserId,
            operation,
            requestId,
            fingerprint,
            invitation.RoomId,
            invitation.Id,
            invitation.Status.ToString(),
            revision,
            assigneeSessionId: null,
            assigneeSessionIncarnationId: null,
            _timeProvider.GetUtcNow());
        await _mutationReceipts.AddAsync(receipt, ct);
    }

    private static void ThrowIfInvitationLifecycleConflict(string result)
    {
        if (Enum.TryParse<RoomInvitationStatus>(result, out var status)
            && status is RoomInvitationStatus.Expired or RoomInvitationStatus.Superseded)
        {
            throw new InvitationLifecycleConflictException(status);
        }
    }

    public async Task<IReadOnlyList<RoomInvitationWithContext>> GetIncomingAsync(
        UserId userId,
        CancellationToken ct = default)
    {
        var raw = await _invitations.GetIncomingAsync(userId, InvitationListHardLimit, ct);
        return await EnrichAsync(raw, ct);
    }

    public async Task<IReadOnlyList<RoomInvitationWithContext>> GetOutgoingAsync(
        UserId userId,
        CancellationToken ct = default)
    {
        var raw = await _invitations.GetOutgoingAsync(userId, InvitationListHardLimit, ct);
        return await EnrichAsync(raw, ct);
    }

    public async Task<RoomInvitationWithContext> GetContextAsync(
        RoomInvitation invitation,
        CancellationToken ct = default)
    {
        var enriched = await EnrichAsync([invitation], ct);
        return enriched[0];
    }

    private async Task<IReadOnlyList<RoomInvitationWithContext>> EnrichAsync(
        IReadOnlyList<RoomInvitation> raw,
        CancellationToken ct)
    {
        if (raw.Count == 0)
        {
            return [];
        }
        var userIds = raw
            .SelectMany(i => new[] { i.InvitedByUserId, i.InviteeUserId })
            .Distinct()
            .ToList();
        var usersById = (await _users.GetByIdsAsync(userIds, ct))
            .ToDictionary(u => u.Id);
        var roomsById = (await _rooms.GetByIdsAsync(
                raw.Select(invitation => invitation.RoomId).Distinct().ToList(),
                ct))
            .ToDictionary(room => room.Id);
        var result = new List<RoomInvitationWithContext>(raw.Count);
        foreach (var invitation in raw)
        {
            roomsById.TryGetValue(invitation.RoomId, out var room);
            usersById.TryGetValue(invitation.InvitedByUserId, out var inviter);
            usersById.TryGetValue(invitation.InviteeUserId, out var invitee);
            result.Add(new RoomInvitationWithContext(
                invitation,
                room?.Name ?? "(unknown room)",
                room?.Slug ?? string.Empty,
                inviter?.Handle ?? "(unknown user)",
                inviter?.DisplayName,
                invitee?.Handle ?? "(unknown user)",
                invitee?.DisplayName));
        }
        return result;
    }

    private async Task AcquireLifecycleLocksAsync(
        IEnumerable<UserId> userIds,
        CancellationToken ct)
    {
        foreach (var userId in userIds
            .Distinct()
            .OrderBy(userId => userId.Value))
        {
            await _userLifecycleLock.AcquireAsync(userId, ct);
        }
    }

}

public enum MembershipTransition
{
    Added,
    Restored,
    NoOp,
}

public sealed record RoomInvitationWithContext(
    RoomInvitation Invitation,
    string RoomName,
    string RoomSlug,
    string InvitedByHandle,
    string? InvitedByDisplayName,
    string InviteeHandle,
    string? InviteeDisplayName);

public sealed class InvitationLifecycleConflictException : ConflictException
{
    public InvitationLifecycleConflictException(RoomInvitationStatus status)
        : base(MessageFor(status), CodeFor(status))
    {
        Status = status;
    }

    public RoomInvitationStatus Status { get; }

    private static string MessageFor(RoomInvitationStatus status) => status switch
    {
        RoomInvitationStatus.Expired =>
            "Invitation proposal has expired; request a new invitation.",
        RoomInvitationStatus.Superseded =>
            "Room roster changed after this invitation was proposed; re-invite the member.",
        _ => throw new ArgumentOutOfRangeException(nameof(status)),
    };

    private static string CodeFor(RoomInvitationStatus status) => status switch
    {
        RoomInvitationStatus.Expired => "INVITATION_EXPIRED",
        RoomInvitationStatus.Superseded => "INVITATION_SUPERSEDED",
        _ => throw new ArgumentOutOfRangeException(nameof(status)),
    };
}

public sealed record RoomInvitationMutationResult(
    RoomMutationReceiptResult Receipt,
    RoomInvitation Invitation,
    MembershipTransition MembershipTransition);
