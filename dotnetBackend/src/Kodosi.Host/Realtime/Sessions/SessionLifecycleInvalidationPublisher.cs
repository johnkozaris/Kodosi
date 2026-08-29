using Kodosi.Application;

namespace Kodosi.Host.Realtime;

internal static class SessionLifecycleInvalidationPublisher
{
    public static async Task TryPublishAsync(
        SharedSurfaceEventPublisher sharedSurfaceEvents,
        LiveSessionTransitionResult transition,
        ILogger logger,
        CancellationToken ct)
    {
        if (!transition.Outcome.SatisfiesTargetState() || transition.SharingState is null)
        {
            return;
        }

        try
        {
            await sharedSurfaceEvents.PublishSessionLifecycleChangedAsync(transition.SharingState, ct);
        }
        catch (Exception ex)
        {
            logger.LogError(
                ex,
                "Failed to publish lifecycle invalidation for session {SessionId}",
                transition.SharingState.SessionId);
        }
    }
}
