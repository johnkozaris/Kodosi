using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Realtime;

namespace Kodosi.Host.Identity;

internal sealed class DeviceEnrollmentPublisher(
    IIdentityExposureRepository identityExposures,
    UserEventBroadcaster broadcaster,
    ILogger<DeviceEnrollmentPublisher> logger)
{
    public async Task PublishAsync(
        UserId userId,
        DeviceEnrollmentResult result)
    {
        try
        {
            var historicalPeers = await identityExposures.GetHistoricalPeerUserIdsAsync(
                userId,
                CancellationToken.None);
            var audience = DiscoveryAudience.ForUsers(
                historicalPeers.Append(userId));
            if (result.BootstrappedIdentity)
            {
                broadcaster.PublishIdentityLifecycleChanged(
                    audience,
                    userId,
                    result.IdentityRevision,
                    result.IdentityIncarnationId,
                    result.DeviceListGeneration);
            }
            else
            {
                broadcaster.PublishDeviceListChanged(
                    audience,
                    userId,
                    result.DeviceListGeneration);
            }
        }
        catch (Exception error)
        {
            logger.LogError(
                error,
                "device_registration audience publication failed after committed enrollment user={UserId}",
                userId.Value);
        }
    }
}
