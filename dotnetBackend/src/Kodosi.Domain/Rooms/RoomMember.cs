
namespace Kodosi.Domain;

public sealed class RoomMember
{
    public RoomId RoomId { get; private set; }
    public UserId UserId { get; private set; }
    public RoomRole Role { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }
    public DateTimeOffset? RevokedAt { get; private set; }
    public Guid? AdmissionInvitationId { get; private set; }
    public byte[]? AdmissionProposalBody { get; private set; }
    public byte[]? AdmissionProposalSignature { get; private set; }
    public string? AdmissionProposalSignerDeviceId { get; private set; }
    public byte[]? AdmissionProposalHash { get; private set; }
    public byte[]? AdmissionDecisionBody { get; private set; }
    public byte[]? AdmissionDecisionSignature { get; private set; }
    public string? AdmissionDecisionSignerDeviceId { get; private set; }
    public DateTimeOffset? AdmissionExpiresAt { get; private set; }



    public uint Version { get; private set; }

    public bool IsActive => RevokedAt is null;

    private RoomMember() { }

    public static RoomMember CreateOwner(RoomId roomId, UserId userId)
    {
        return new RoomMember
        {
            RoomId = roomId,
            UserId = userId,
            Role = RoomRole.Owner,
            CreatedAt = DateTimeOffset.UtcNow,
        };
    }

    public static RoomMember CreateFromInvitation(RoomInvitation invitation)
    {
        var member = new RoomMember
        {
            RoomId = invitation.RoomId,
            UserId = invitation.InviteeUserId,
            Role = RoomRole.Member,
            CreatedAt = DateTimeOffset.UtcNow,
        };
        member.ApplyAdmissionProof(invitation);
        return member;
    }

    public void Revoke()
    {
        RevokedAt = DateTimeOffset.UtcNow;
    }

    public void ApplyAcceptedInvitation(RoomInvitation invitation)
    {
        ApplyAdmissionProof(invitation);
        CreatedAt = DateTimeOffset.UtcNow;
        RevokedAt = null;
    }

    private void ApplyAdmissionProof(RoomInvitation invitation)
    {
        if (invitation.Status != RoomInvitationStatus.Accepted
            || invitation.RoomId != RoomId
            || invitation.InviteeUserId != UserId
            || invitation.DecisionBody is not { Length: > 0 }
            || invitation.DecisionSignature is not { Length: > 0 }
            || string.IsNullOrWhiteSpace(invitation.DecisionSignerDeviceId))
        {
            throw new DomainException("Accepted invitation proof is required for room admission.");
        }

        AdmissionInvitationId = invitation.Id;
        AdmissionProposalBody = invitation.ProposalBody.ToArray();
        AdmissionProposalSignature = invitation.ProposalSignature.ToArray();
        AdmissionProposalSignerDeviceId = invitation.ProposalSignerDeviceId;
        AdmissionProposalHash = invitation.ProposalHash.ToArray();
        AdmissionDecisionBody = invitation.DecisionBody.ToArray();
        AdmissionDecisionSignature = invitation.DecisionSignature.ToArray();
        AdmissionDecisionSignerDeviceId = invitation.DecisionSignerDeviceId;
        AdmissionExpiresAt = invitation.ExpiresAt;
    }
}
