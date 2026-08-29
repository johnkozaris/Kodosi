
namespace Kodosi.Application;

public sealed record OwnedSessionCreateResult(
    SessionDetailResponse Response,
    SessionDiscoveryTarget SharingState);
