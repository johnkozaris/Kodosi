namespace Kodosi.Domain;

public sealed class RoomInvitation
{
    public Guid Id { get; private set; }
    public RoomId RoomId { get; private set; } = default!;
    public UserId InviteeUserId { get; private set; } = default!;
    public UserId InvitedByUserId { get; private set; } = default!;
    public RoomInvitationStatus Status { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }
    public DateTimeOffset? RespondedAt { get; private set; }
    public long BaseRosterGeneration { get; private set; }
    public long ProposedRosterGeneration { get; private set; }
    public byte[] ProposedRosterBody { get; private set; } = [];
    public byte[] ProposedRosterSignature { get; private set; } = [];
    public string ProposedRosterSignerDeviceId { get; private set; } = string.Empty;
    public byte[] ProposalBody { get; private set; } = [];
    public byte[] ProposalSignature { get; private set; } = [];
    public string ProposalSignerDeviceId { get; private set; } = string.Empty;
    public byte[] ProposalHash { get; private set; } = [];
    public DateTimeOffset ProposalIssuedAt { get; private set; }
    public DateTimeOffset ExpiresAt { get; private set; }
    public byte[]? DecisionBody { get; private set; }
    public byte[]? DecisionSignature { get; private set; }
    public string? DecisionSignerDeviceId { get; private set; }
    public DateTimeOffset? DecisionIssuedAt { get; private set; }
    public uint Version { get; private set; }

    private RoomInvitation() { }

    public static RoomInvitation Create(
        Guid id,
        RoomId roomId,
        UserId inviteeUserId,
        UserId invitedByUserId,
        RoomInvitationProposalProof proof,
        DateTimeOffset createdAt)
    {
        if (id == Guid.Empty)
        {
            throw new DomainException("Invitation ID is required.");
        }
        if (inviteeUserId == invitedByUserId)
        {
            throw new DomainException("Cannot invite yourself to a room.");
        }
        if (proof.BaseRosterGeneration < 1
            || proof.BaseRosterGeneration == long.MaxValue
            || proof.ProposedRosterGeneration != proof.BaseRosterGeneration + 1
            || proof.ProposedRosterBody.Length == 0
            || proof.ProposedRosterSignature.Length == 0
            || string.IsNullOrWhiteSpace(proof.ProposedRosterSignerDeviceId)
            || proof.ProposalBody.Length == 0
            || proof.ProposalSignature.Length == 0
            || string.IsNullOrWhiteSpace(proof.ProposalSignerDeviceId)
            || proof.ProposalHash.Length == 0
            || proof.ProposalIssuedAt <= DateTimeOffset.UnixEpoch
            || proof.ExpiresAt <= proof.ProposalIssuedAt)
        {
            throw new DomainException("Invitation proposal proof is invalid.");
        }

        return new RoomInvitation
        {
            Id = id,
            RoomId = roomId,
            InviteeUserId = inviteeUserId,
            InvitedByUserId = invitedByUserId,
            Status = RoomInvitationStatus.Pending,
            CreatedAt = createdAt,
            RespondedAt = null,
            BaseRosterGeneration = proof.BaseRosterGeneration,
            ProposedRosterGeneration = proof.ProposedRosterGeneration,
            ProposedRosterBody = proof.ProposedRosterBody.ToArray(),
            ProposedRosterSignature = proof.ProposedRosterSignature.ToArray(),
            ProposedRosterSignerDeviceId = proof.ProposedRosterSignerDeviceId,
            ProposalBody = proof.ProposalBody.ToArray(),
            ProposalSignature = proof.ProposalSignature.ToArray(),
            ProposalSignerDeviceId = proof.ProposalSignerDeviceId,
            ProposalHash = proof.ProposalHash.ToArray(),
            ProposalIssuedAt = proof.ProposalIssuedAt,
            ExpiresAt = proof.ExpiresAt,
        };
    }

    public bool MatchesCreation(
        RoomId roomId,
        UserId inviteeUserId,
        UserId invitedByUserId,
        byte[] proposalBody,
        byte[] proposalSignature,
        string proposalSignerDeviceId,
        long proposedRosterGeneration,
        byte[] proposedRosterBody,
        byte[] proposedRosterSignature,
        string proposedRosterSignerDeviceId) =>
        RoomId == roomId
        && InviteeUserId == inviteeUserId
        && InvitedByUserId == invitedByUserId
        && ProposalBody.AsSpan().SequenceEqual(proposalBody)
        && ProposalSignature.AsSpan().SequenceEqual(proposalSignature)
        && string.Equals(
            ProposalSignerDeviceId,
            proposalSignerDeviceId,
            StringComparison.Ordinal)
        && ProposedRosterGeneration == proposedRosterGeneration
        && ProposedRosterBody.AsSpan().SequenceEqual(proposedRosterBody)
        && ProposedRosterSignature.AsSpan().SequenceEqual(proposedRosterSignature)
        && string.Equals(
            ProposedRosterSignerDeviceId,
            proposedRosterSignerDeviceId,
            StringComparison.Ordinal);

    public bool IsExpired(DateTimeOffset now) => now >= ExpiresAt;

    public void Accept(
        UserId actingUserId,
        RoomInvitationDecisionProof proof,
        DateTimeOffset respondedAt)
        => ResolveByInvitee(
            actingUserId,
            RoomInvitationStatus.Accepted,
            "accept",
            proof,
            respondedAt);

    public void Decline(
        UserId actingUserId,
        RoomInvitationDecisionProof proof,
        DateTimeOffset respondedAt)
        => ResolveByInvitee(
            actingUserId,
            RoomInvitationStatus.Declined,
            "decline",
            proof,
            respondedAt);

    private void ResolveByInvitee(
        UserId actingUserId,
        RoomInvitationStatus to,
        string verb,
        RoomInvitationDecisionProof proof,
        DateTimeOffset respondedAt)
    {
        if (actingUserId != InviteeUserId)
        {
            throw new DomainException($"Only the invitee may {verb} this invitation.");
        }

        EnsurePending();
        if (IsExpired(respondedAt))
        {
            throw new InvalidStateException("Invitation proposal has expired.");
        }
        if (proof.Body.Length == 0
            || proof.Signature.Length == 0
            || string.IsNullOrWhiteSpace(proof.SignerDeviceId)
            || proof.IssuedAt <= DateTimeOffset.UnixEpoch
            || proof.IssuedAt > ExpiresAt)
        {
            throw new DomainException("Invitation decision proof is invalid.");
        }

        Status = to;
        RespondedAt = respondedAt;
        DecisionBody = proof.Body.ToArray();
        DecisionSignature = proof.Signature.ToArray();
        DecisionSignerDeviceId = proof.SignerDeviceId;
        DecisionIssuedAt = proof.IssuedAt;
    }



    public void CancelByInviter(UserId actingUserId, DateTimeOffset respondedAt)
    {
        if (actingUserId != InvitedByUserId)
        {
            throw new DomainException("Only the inviter may cancel this invitation.");
        }

        EnsurePending();
        Status = RoomInvitationStatus.Cancelled;
        RespondedAt = respondedAt;
    }

    public void Expire(DateTimeOffset respondedAt)
    {
        EnsurePending();
        Status = RoomInvitationStatus.Expired;
        RespondedAt = respondedAt;
    }

    public void Supersede(DateTimeOffset respondedAt)
    {
        EnsurePending();
        Status = RoomInvitationStatus.Superseded;
        RespondedAt = respondedAt;
    }

    private void EnsurePending()
    {
        if (Status != RoomInvitationStatus.Pending)
        {
            throw new InvalidStateException(
                $"Invitation is already {Status}; no further transition allowed.");
        }
    }
}

public sealed record RoomInvitationProposalProof(
    long BaseRosterGeneration,
    long ProposedRosterGeneration,
    byte[] ProposedRosterBody,
    byte[] ProposedRosterSignature,
    string ProposedRosterSignerDeviceId,
    byte[] ProposalBody,
    byte[] ProposalSignature,
    string ProposalSignerDeviceId,
    byte[] ProposalHash,
    DateTimeOffset ProposalIssuedAt,
    DateTimeOffset ExpiresAt);

public sealed record RoomInvitationDecisionProof(
    byte[] Body,
    byte[] Signature,
    string SignerDeviceId,
    DateTimeOffset IssuedAt);
