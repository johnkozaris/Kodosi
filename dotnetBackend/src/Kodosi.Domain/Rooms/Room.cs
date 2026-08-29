
namespace Kodosi.Domain;

public sealed class Room
{
    public RoomId Id { get; private set; }
    public UserId OwnerUserId { get; private set; }
    public string Name { get; private set; } = string.Empty;
    public string Slug { get; private set; } = string.Empty;
    public DateTimeOffset CreatedAt { get; private set; }
    public long RosterGeneration { get; private set; }
    public byte[] RosterBody { get; private set; } = [];
    public byte[] RosterSignature { get; private set; } = [];
    public string RosterSignerDeviceId { get; private set; } = string.Empty;
    public Guid? RosterActivationInvitationId { get; private set; }
    public UserId? RosterActivationInviteeUserId { get; private set; }
    public byte[]? RosterActivationProposalBody { get; private set; }
    public byte[]? RosterActivationProposalSignature { get; private set; }
    public string? RosterActivationProposalSignerDeviceId { get; private set; }
    public byte[]? RosterActivationProposalHash { get; private set; }
    public byte[]? RosterActivationDecisionBody { get; private set; }
    public byte[]? RosterActivationDecisionSignature { get; private set; }
    public string? RosterActivationDecisionSignerDeviceId { get; private set; }
    public DateTimeOffset? RosterActivationExpiresAt { get; private set; }
    public uint Version { get; private set; }

    private Room() { }

    public bool IsOwner(UserId userId) => OwnerUserId == userId;

    public static Room Create(
        RoomId id,
        UserId ownerUserId,
        string name,
        string slug,
        long rosterGeneration,
        byte[] rosterBody,
        byte[] rosterSignature,
        string rosterSignerDeviceId)
    {
        if (rosterGeneration != 1
            || rosterBody.Length == 0
            || rosterSignature.Length == 0
            || string.IsNullOrWhiteSpace(rosterSignerDeviceId))
        {
            throw new DomainException("A room requires a signed generation-1 roster.");
        }
        return new Room
        {
            Id = id,
            OwnerUserId = ownerUserId,
            Name = name,
            Slug = slug,
            CreatedAt = DateTimeOffset.UtcNow,
            RosterGeneration = rosterGeneration,
            RosterBody = rosterBody,
            RosterSignature = rosterSignature,
            RosterSignerDeviceId = rosterSignerDeviceId,
        };
    }

    public void ReplaceRosterForRemoval(
        long generation,
        byte[] body,
        byte[] signature,
        string signerDeviceId)
    {
        if (RosterGeneration == long.MaxValue || generation != RosterGeneration + 1)
        {
            throw new DomainException("Room roster generation must increase by exactly one.");
        }
        if (body.Length == 0 || signature.Length == 0 || string.IsNullOrWhiteSpace(signerDeviceId))
        {
            throw new DomainException("Signed room roster fields are required.");
        }
        RosterGeneration = generation;
        RosterBody = body;
        RosterSignature = signature;
        RosterSignerDeviceId = signerDeviceId;
        ClearRosterActivationProof();
    }

    public void ActivateInvitation(RoomInvitation invitation)
    {
        if (invitation.Status != RoomInvitationStatus.Accepted
            || invitation.RoomId != Id
            || invitation.InvitedByUserId != OwnerUserId
            || invitation.BaseRosterGeneration != RosterGeneration
            || RosterGeneration == long.MaxValue
            || invitation.ProposedRosterGeneration != RosterGeneration + 1
            || invitation.DecisionBody is not { Length: > 0 }
            || invitation.DecisionSignature is not { Length: > 0 }
            || string.IsNullOrWhiteSpace(invitation.DecisionSignerDeviceId))
        {
            throw new DomainException(
                "Accepted invitation proof does not authorize this roster transition.");
        }

        RosterGeneration = invitation.ProposedRosterGeneration;
        RosterBody = invitation.ProposedRosterBody.ToArray();
        RosterSignature = invitation.ProposedRosterSignature.ToArray();
        RosterSignerDeviceId = invitation.ProposedRosterSignerDeviceId;
        RosterActivationInvitationId = invitation.Id;
        RosterActivationInviteeUserId = invitation.InviteeUserId;
        RosterActivationProposalBody = invitation.ProposalBody.ToArray();
        RosterActivationProposalSignature = invitation.ProposalSignature.ToArray();
        RosterActivationProposalSignerDeviceId = invitation.ProposalSignerDeviceId;
        RosterActivationProposalHash = invitation.ProposalHash.ToArray();
        RosterActivationDecisionBody = invitation.DecisionBody.ToArray();
        RosterActivationDecisionSignature = invitation.DecisionSignature.ToArray();
        RosterActivationDecisionSignerDeviceId = invitation.DecisionSignerDeviceId;
        RosterActivationExpiresAt = invitation.ExpiresAt;
    }

    private void ClearRosterActivationProof()
    {
        RosterActivationInvitationId = null;
        RosterActivationInviteeUserId = null;
        RosterActivationProposalBody = null;
        RosterActivationProposalSignature = null;
        RosterActivationProposalSignerDeviceId = null;
        RosterActivationProposalHash = null;
        RosterActivationDecisionBody = null;
        RosterActivationDecisionSignature = null;
        RosterActivationDecisionSignerDeviceId = null;
        RosterActivationExpiresAt = null;
    }
}
