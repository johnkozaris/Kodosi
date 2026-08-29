using Kodosi.Domain;
using Kodosi.Host.Realtime;

namespace Kodosi.Host.Identity;

internal sealed class DeviceLinkRequestPublisher(
    UserEventBroadcaster userEvents,
    ILogger<DeviceLinkRequestPublisher> logger)
{
    public void PublishRequested(
        UserId userId,
        string userCode,
        string deviceLabel,
        DateTimeOffset expiresAt)
    {
        try
        {
            userEvents.PublishDeviceLinkRequested(
                userId,
                userCode,
                deviceLabel,
                expiresAt);
        }
        catch (Exception error)
        {
            logger.LogError(
                error,
                "device_link request publication failed after commit user={UserId} code={UserCode}",
                userId.Value,
                userCode);
        }
    }

    public void PublishCancelled(UserId userId, string userCode)
    {
        try
        {
            userEvents.PublishDeviceLinkResolved(userId, userCode, "cancelled");
        }
        catch (Exception error)
        {
            logger.LogError(
                error,
                "device_link cancellation publication failed after commit user={UserId} code={UserCode}",
                userId.Value,
                userCode);
        }
    }
}
