using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record UserSummaryResponse(
    Guid Id,
    string Handle,
    string DisplayName,
    string? AvatarUrl);

public sealed record FriendRequestResponse(
    Guid UserId,
    string Handle,
    string DisplayName,
    string? AvatarUrl,
    DateTimeOffset CreatedAt);

public sealed record FriendRequestInboxResponse(
    IReadOnlyList<FriendRequestResponse> Incoming,
    IReadOnlyList<FriendRequestResponse> Outgoing);

public sealed record UserProfileResponse(
    Guid Id,
    string Handle,
    string DisplayName,
    string? Email,
    string? AvatarUrl);

public sealed record RoomResponse(
    Guid Id,
    string Name,
    string Slug,
    Guid OwnerUserId,
    long RosterGeneration,
    byte[] RosterBody,
    byte[] RosterSignature,
    string RosterSignerDeviceId,
    RoomRosterActivationProofResponse? RosterActivationProof,
    IReadOnlyList<RoomAdmissionProofResponse> AdmissionProofs,
    IReadOnlyList<RoomRosterTransitionResponse> RosterTransitions);

public sealed record RoomRosterTransitionResponse(
    long Generation,
    byte[] RosterBody,
    byte[] RosterSignature,
    string RosterSignerDeviceId,
    Guid? AdmissionInvitationId);

public sealed record RoomRosterActivationProofResponse(
    Guid InvitationId,
    Guid InviteeUserId,
    byte[] ProposalBody,
    byte[] ProposalSignature,
    string ProposalSignerDeviceId,
    byte[] ProposalHash,
    byte[] DecisionBody,
    byte[] DecisionSignature,
    string DecisionSignerDeviceId,
    DateTimeOffset ExpiresAt);

public sealed record RoomAdmissionProofResponse(
    Guid InvitationId,
    Guid InviteeUserId,
    byte[] ProposalBody,
    byte[] ProposalSignature,
    string ProposalSignerDeviceId,
    byte[] ProposalHash,
    byte[] DecisionBody,
    byte[] DecisionSignature,
    string DecisionSignerDeviceId,
    DateTimeOffset ExpiresAt);

public sealed record HealthStatusResponse(
    string Status,
    int ApiContractVersion,
    int AuthContractVersion);

public sealed record RoomMemberSummaryResponse(
    Guid RoomId,
    Guid UserId,
    RoomRole Role,
    string Username,
    string DisplayName,
    string? AvatarUrl);

public sealed record SessionAccessGrantResponse(
    Guid ActorUserId,
    string Handle,
    string DisplayName,
    AccessLevel AccessLevel,
    DateTimeOffset GrantedAt,
    DateTimeOffset? ExpiresAt);

public sealed record SessionAccessGrantsResponse(
    Guid IncarnationId,
    IReadOnlyList<SessionAccessGrantResponse> Grants);

public sealed record AuthorizedDeviceResponse(
    Guid UserId,
    string DeviceId);

public sealed record UserIdentityBundleResponse(
    Guid UserId,
    long IdentityRevision,
    Guid IdentityIncarnationId,
    UserDeviceListResponse DeviceList,
    IReadOnlyList<UserDeviceCertificateResponse> Devices,
    IReadOnlyList<UserDeviceCertificateResponse> HistoricalDevices);

public sealed record UserDeviceListResponse(
    string Body,
    string Signature);

public sealed record UserDeviceCertificateResponse(
    string Certificate,
    string CertificateSignature);

public sealed record DeviceRegistrationChallengeResponse(
    Guid ChallengeId,
    string ChallengeBytes,
    DateTimeOffset ExpiresAt);

public enum SessionKeyFetchState
{
    Ready,
    PendingDistribution,
    UnknownDevice,
    SessionNotLive,
}

public sealed record SessionKeyBlobResponse(
    Guid IncarnationId,
    int IncarnationProtocolVersion,
    string EncryptedSessionKey,
    string SenderDeviceId,
    string SenderKemPublicKey,
    string? Signature,
    int SignatureVersion,
    int KeyGeneration,

    long IssuedAtMs);

public sealed record SessionKeyFetchResponse(
    SessionKeyFetchState State,
    SessionKeyBlobResponse? KeyBlob = null);

public enum SessionKeyGenerationClaimResponseState
{
    Claimed,
    GenerationChanged,
}

public sealed record SessionKeyGenerationClaimResponse(
    SessionKeyGenerationClaimResponseState State,
    int Generation);

public sealed record CurrentKeyGenerationResponse(int CurrentGeneration);
