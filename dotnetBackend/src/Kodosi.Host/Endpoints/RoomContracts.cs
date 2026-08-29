using System.ComponentModel.DataAnnotations;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Middleware;

namespace Kodosi.Host.Endpoints;

internal static class RoomRequestLimits
{
    private const long Base64EncodedRosterBytes =
        ((RoomInputRules.EncryptedContentMaxLength + 2L) / 3L) * 4L;

    public const long EncryptedContentBodyBytes =
        (2L * RoomInputRules.EncryptedContentMaxLength) + (64 * 1024);


    public const long SingleRosterBodyBytes =
        Base64EncodedRosterBytes + (64 * 1024);


    public const long InvitationWithRosterBodyBytes =
        (2L * Base64EncodedRosterBytes) + (128 * 1024);
}

public sealed record CreateRoomRequest(
    [property: NotEmptyGuid]
    Guid RoomId,
    [property: Required, StringLength(RoomInputRules.RoomNameMaxLength, MinimumLength = 1)]
    string Name,
    [property: Required, RegularExpression(RoomInputRules.RoomSlugPattern)]
    string Slug,
    long RosterGeneration,
    [property: Required, Base64String] string RosterBody,
    [property: Required, Base64String] string RosterSignature,
    [property: Required] string RosterSignerDeviceId);

public sealed record InviteRoomMemberRequest(
    [property: NotEmptyGuid]
    Guid InvitationId,
    [property: NotEmptyGuid]
    Guid InviteeUserId,
    [property: Required, Base64String] string ProposalBody,
    [property: Required, Base64String] string ProposalSignature,
    [property: Required] string ProposalSignerDeviceId,
    long ProposedRosterGeneration,
    [property: Required, Base64String] string ProposedRosterBody,
    [property: Required, Base64String] string ProposedRosterSignature,
    [property: Required] string ProposedRosterSignerDeviceId);

public sealed record RoomInvitationDecisionRequest(
    [property: UuidV7] Guid RequestId,
    [property: Required, Base64String] string DecisionBody,
    [property: Required, Base64String] string DecisionSignature,
    [property: Required] string DecisionSignerDeviceId);

public sealed record CancelRoomInvitationRequest(
    [property: UuidV7] Guid RequestId);

public sealed record ReplaceRoomRosterRequest(
    [property: UuidV7] Guid RequestId,
    long RosterGeneration,
    [property: Required, Base64String] string RosterBody,
    [property: Required, Base64String] string RosterSignature,
    [property: Required] string RosterSignerDeviceId);

public sealed record RoomMutationReceiptResponse(
    Guid RequestId,
    string Operation,
    Guid RoomId,
    Guid EntityId,
    string Result,
    long? Revision,
    Guid? AssigneeSessionId,
    Guid? AssigneeSessionIncarnationId,
    string TargetFingerprint,
    DateTimeOffset CreatedAt);

public sealed record RoomMutationResponse(
    Guid RequestId,
    string Operation,
    Guid RoomId,
    Guid EntityId,
    string Result,
    long? Revision,
    Guid? AssigneeSessionId,
    Guid? AssigneeSessionIncarnationId);

public sealed record RoomInvitationResponse(
    Guid Id,
    Guid RoomId,
    string RoomName,
    string RoomSlug,
    Guid InviteeUserId,
    string InviteeHandle,
    string? InviteeDisplayName,
    Guid InvitedByUserId,
    string InvitedByHandle,
    string? InvitedByDisplayName,
    string Status,
    DateTimeOffset CreatedAt,
    DateTimeOffset? RespondedAt,
    long BaseRosterGeneration,
    long ProposedRosterGeneration,
    string ProposedRosterBody,
    string ProposedRosterSignature,
    string ProposedRosterSignerDeviceId,
    string ProposalBody,
    string ProposalSignature,
    string ProposalSignerDeviceId,
    string ProposalHash,
    DateTimeOffset ProposalIssuedAt,
    DateTimeOffset ExpiresAt,
    string? DecisionBody,
    string? DecisionSignature,
    string? DecisionSignerDeviceId,
    DateTimeOffset? DecisionIssuedAt);

public sealed record PostRoomChatRequest(
    Guid MessageId,
    [property: Required, StringLength(RoomInputRules.EncryptedContentMaxLength, MinimumLength = 1)]
    string Body,
    Guid? AuthorSessionId,
    [property: EnumDataType(typeof(RoomChatAuthorKind))]
    RoomChatAuthorKind AuthorKind,
    IReadOnlyList<Guid>? RecipientSessionIds = null,
    IReadOnlyList<Guid>? RecipientUserIds = null);

public sealed record RoomChatMessageResponse(
    Guid Id,
    Guid RoomId,
    Guid AuthorUserId,
    Guid? AuthorSessionId,
    RoomChatAuthorKind AuthorKind,
    IReadOnlyList<Guid> RecipientSessionIds,
    IReadOnlyList<Guid> RecipientUserIds,
    string Body,
    long Seq,
    DateTimeOffset PostedAt);

public sealed record CreateRoomTaskRequest(
    Guid TaskId,
    [property: Required, StringLength(RoomInputRules.TaskTitleMaxLength, MinimumLength = 1)]
    string Title,
    [property: StringLength(RoomInputRules.TaskDescriptionMaxLength)]
    string? Description,
    Guid? AssignedSessionId,
    Guid? AssignedSessionIncarnationId,
    DateTimeOffset? DueAt);

public sealed record UpdateRoomTaskStatusRequest(
    [property: UuidV7] Guid RequestId,
    long ExpectedTaskRevision,
    [property: Required]
    string Status,
    Guid? ActorSessionId,
    Guid? ActorSessionIncarnationId,
    [property: StringLength(RoomInputRules.TaskResultMaxLength)]
    string? Result);

public sealed record AssignRoomTaskRequest(
    [property: UuidV7] Guid RequestId,
    long ExpectedTaskRevision,
    Guid? SessionId,
    Guid? SessionIncarnationId);

public sealed record RoomTaskResponse(
    Guid Id,
    Guid RoomId,
    Guid CreatedByUserId,
    string Title,
    string? Description,
    string Status,
    Guid? AssignedSessionId,
    Guid? AssignedSessionIncarnationId,
    DateTimeOffset? DueAt,
    DateTimeOffset CreatedAt,
    DateTimeOffset UpdatedAt,
    DateTimeOffset? CompletedAt,
    string? Result,
    Guid? ResultAuthorUserId,
    long Revision);

internal static class RoomResponseMappers
{
    public static RoomMutationReceiptResponse MapMutationReceipt(
        RoomMutationReceiptSnapshot receipt) =>
        new(
            receipt.RequestId,
            receipt.Operation,
            receipt.RoomId.Value,
            receipt.EntityId,
            receipt.Result,
            receipt.Revision,
            receipt.AssigneeSessionId,
            receipt.AssigneeSessionIncarnationId,
            Convert.ToHexStringLower(receipt.TargetFingerprint.ToArray()),
            receipt.CreatedAt);

    public static RoomMutationResponse MapMutation(
        Guid requestId,
        string operation,
        RoomMutationReceiptResult receipt) =>
        new(
            requestId,
            operation,
            receipt.RoomId.Value,
            receipt.EntityId,
            receipt.Result,
            receipt.Revision,
            receipt.AssigneeSessionId,
            receipt.AssigneeSessionIncarnationId);

    public static RoomResponse MapRoom(
        Room room,
        IReadOnlyList<RoomMember> admissionMembers,
        IReadOnlyList<RoomRosterTransition>? rosterTransitions = null)
    {
        RoomRosterActivationProofResponse? activationProof = null;
        if (room.RosterActivationInvitationId is { } invitationId
            && room.RosterActivationInviteeUserId is { } inviteeUserId
            && room.RosterActivationProposalBody is { Length: > 0 } proposalBody
            && room.RosterActivationProposalSignature is { Length: > 0 } proposalSignature
            && room.RosterActivationProposalSignerDeviceId is { Length: > 0 } proposalSigner
            && room.RosterActivationProposalHash is { Length: > 0 } proposalHash
            && room.RosterActivationDecisionBody is { Length: > 0 } decisionBody
            && room.RosterActivationDecisionSignature is { Length: > 0 } decisionSignature
            && room.RosterActivationDecisionSignerDeviceId is { Length: > 0 } decisionSigner
            && room.RosterActivationExpiresAt is { } expiresAt)
        {
            activationProof = new RoomRosterActivationProofResponse(
                invitationId,
                inviteeUserId.Value,
                proposalBody,
                proposalSignature,
                proposalSigner,
                proposalHash,
                decisionBody,
                decisionSignature,
                decisionSigner,
                expiresAt);
        }

        var admissionProofs = admissionMembers
            .Where(member =>
                member.RoomId == room.Id
                && member.UserId != room.OwnerUserId
                && member.IsActive)
            .Select(MapAdmissionProof)
            .OrderBy(proof => proof.InviteeUserId)
            .ToList();

        return new(
            room.Id.Value,
            room.Name,
            room.Slug,
            room.OwnerUserId.Value,
            room.RosterGeneration,
            room.RosterBody,
            room.RosterSignature,
            room.RosterSignerDeviceId,
            activationProof,
            admissionProofs,
            (rosterTransitions ?? [])
                .Where(transition => transition.RoomId == room.Id)
                .OrderBy(transition => transition.Generation)
                .Select(MapRosterTransition)
                .ToList());
    }

    public static RoomRosterTransitionResponse MapRosterTransition(
        RoomRosterTransition transition) =>
        new(
            transition.Generation,
            transition.RosterBody,
            transition.RosterSignature,
            transition.RosterSignerDeviceId,
            transition.AdmissionInvitationId);

    public static RoomAdmissionProofResponse MapAdmissionProof(RoomMember member)
    {
        if (member.AdmissionInvitationId is not { } invitationId
            || member.AdmissionProposalBody is not { Length: > 0 } proposalBody
            || member.AdmissionProposalSignature is not { Length: > 0 } proposalSignature
            || string.IsNullOrWhiteSpace(member.AdmissionProposalSignerDeviceId)
            || member.AdmissionProposalHash is not { Length: > 0 } proposalHash
            || member.AdmissionDecisionBody is not { Length: > 0 } decisionBody
            || member.AdmissionDecisionSignature is not { Length: > 0 } decisionSignature
            || string.IsNullOrWhiteSpace(member.AdmissionDecisionSignerDeviceId)
            || member.AdmissionExpiresAt is not { } expiresAt)
        {
            throw new InvalidStateException(
                $"Active room member {member.UserId} has no durable admission proof.");
        }

        return new RoomAdmissionProofResponse(
            invitationId,
            member.UserId.Value,
            proposalBody,
            proposalSignature,
            member.AdmissionProposalSignerDeviceId,
            proposalHash,
            decisionBody,
            decisionSignature,
            member.AdmissionDecisionSignerDeviceId,
            expiresAt);
    }

    public static RoomChatMessageResponse MapChat(RoomChatMessage message) =>
        new(
            message.Id,
            message.RoomId.Value,
            message.AuthorUserId.Value,
            message.AuthorSessionId,
            message.AuthorKind,
            message.RecipientSessionIds,
            message.RecipientUserIds,
            message.Body,
            message.Seq,
            message.PostedAt);

    public static RoomTaskResponse MapTask(RoomTask task) =>
        new(
            task.Id,
            task.RoomId.Value,
            task.CreatedByUserId.Value,
            task.Title,
            task.Description,
            task.Status.ToString(),
            task.AssignedSessionId,
            task.AssignedSessionIncarnationId,
            task.DueAt,
            task.CreatedAt,
            task.UpdatedAt,
            task.CompletedAt,
            task.Result,
            task.ResultAuthorUserId?.Value,
            task.Revision);

    public static RoomInvitationResponse MapInvitation(RoomInvitationWithContext ctx) =>
        new(
            ctx.Invitation.Id,
            ctx.Invitation.RoomId.Value,
            ctx.RoomName,
            ctx.RoomSlug,
            ctx.Invitation.InviteeUserId.Value,
            ctx.InviteeHandle,
            ctx.InviteeDisplayName,
            ctx.Invitation.InvitedByUserId.Value,
            ctx.InvitedByHandle,
            ctx.InvitedByDisplayName,
            ctx.Invitation.Status.ToString(),
            ctx.Invitation.CreatedAt,
            ctx.Invitation.RespondedAt,
            ctx.Invitation.BaseRosterGeneration,
            ctx.Invitation.ProposedRosterGeneration,
            Convert.ToBase64String(ctx.Invitation.ProposedRosterBody),
            Convert.ToBase64String(ctx.Invitation.ProposedRosterSignature),
            ctx.Invitation.ProposedRosterSignerDeviceId,
            Convert.ToBase64String(ctx.Invitation.ProposalBody),
            Convert.ToBase64String(ctx.Invitation.ProposalSignature),
            ctx.Invitation.ProposalSignerDeviceId,
            Convert.ToBase64String(ctx.Invitation.ProposalHash),
            ctx.Invitation.ProposalIssuedAt,
            ctx.Invitation.ExpiresAt,
            ctx.Invitation.DecisionBody is { } decisionBody
                ? Convert.ToBase64String(decisionBody)
                : null,
            ctx.Invitation.DecisionSignature is { } decisionSignature
                ? Convert.ToBase64String(decisionSignature)
                : null,
            ctx.Invitation.DecisionSignerDeviceId,
            ctx.Invitation.DecisionIssuedAt);
}
