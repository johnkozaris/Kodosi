using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Realtime;

namespace Kodosi.Host.Identity;

internal sealed class DeviceLinkRealtimeEffects(
    IFriendshipRepository friendships,
    IRoomMemberRepository roomMembers,
    IIdentityExposureRepository identityExposures,
    UserEventBroadcaster userEvents,
    ILogger<DeviceLinkRealtimeEffects> logger) : IDeviceLinkRealtimeEffects
{
    public async Task PublishApprovedAsync(
        UserId userId,
        long generation,
        string userCode)
    {
        try
        {
            var friendIds = await friendships.GetFriendIdsAsync(
                userId,
                CancellationToken.None);
            var roomPeerIds = await roomMembers.GetRoomPeerUserIdsAsync(
                userId,
                CancellationToken.None);
            var historicalPeers = await identityExposures.GetHistoricalPeerUserIdsAsync(
                userId,
                CancellationToken.None);
            var audience = DeviceListAudience.Build(
                friendIds.Concat(historicalPeers),
                roomPeerIds,
                userId);
            userEvents.PublishDeviceListChanged(
                DiscoveryAudience.ForUsers(audience),
                userId,
                generation);
            userEvents.PublishDeviceLinkResolved(userId, userCode, "approved");
        }
        catch (Exception error)
        {
            logger.LogError(
                error,
                "device_link approval publication failed after commit user={UserId} code={UserCode}",
                userId.Value,
                userCode);
        }
    }
}
