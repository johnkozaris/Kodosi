using System.Text.Json.Serialization;

namespace Kodosi.Application;



[JsonSerializable(typeof(UserSummaryResponse))]
[JsonSerializable(typeof(UserProfileResponse))]
[JsonSerializable(typeof(FriendRequestResponse))]
[JsonSerializable(typeof(FriendRequestInboxResponse))]
[JsonSerializable(typeof(RoomResponse))]
[JsonSerializable(typeof(RoomAdmissionProofResponse))]
[JsonSerializable(typeof(RoomMemberSummaryResponse))]
[JsonSerializable(typeof(SessionAccessGrantResponse))]
[JsonSerializable(typeof(SessionAccessGrantsResponse))]
[JsonSerializable(typeof(AuthorizedDeviceResponse))]
[JsonSerializable(typeof(UserIdentityBundleResponse))]
[JsonSerializable(typeof(UserDeviceListResponse))]
[JsonSerializable(typeof(UserDeviceCertificateResponse))]
[JsonSerializable(typeof(DeviceRegistrationChallengeResponse))]
[JsonSerializable(typeof(SessionKeyFetchResponse))]
[JsonSerializable(typeof(SessionKeyBlobResponse))]
[JsonSerializable(typeof(SessionKeyGenerationClaimResponse))]
[JsonSerializable(typeof(CurrentKeyGenerationResponse))]
[JsonSerializable(typeof(SessionCardResponse))]
[JsonSerializable(typeof(SessionDetailResponse))]
[JsonSerializable(typeof(PaginatedFeedResponse<SessionCardResponse>))]
[JsonSerializable(typeof(HealthStatusResponse))]
[JsonSerializable(typeof(IReadOnlyList<SessionCardResponse>))]
[JsonSerializable(typeof(IReadOnlyList<RoomResponse>))]
[JsonSerializable(typeof(IReadOnlyList<RoomMemberSummaryResponse>))]
[JsonSerializable(typeof(IReadOnlyList<SessionAccessGrantResponse>))]
[JsonSerializable(typeof(IReadOnlyList<AuthorizedDeviceResponse>))]
[JsonSerializable(typeof(CreateSessionRequest))]
[JsonSerializable(typeof(UpdateSessionRequest))]
[JsonSerializable(typeof(FriendRequestHandleRequest))]
public partial class AppJsonContext : JsonSerializerContext
{
}
