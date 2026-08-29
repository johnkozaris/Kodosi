using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

namespace Kodosi.Host.Realtime;

internal static class HostStreamDemandEmitter
{
    public static void TryEmit(
        SessionBroadcaster broadcaster,
        OperationalMetrics metrics,
        ILogger logger,
        SessionId sessionId,
        StreamDemandTransition transition,
        StreamDemandReason reason)
    {
        if (!transition.RequirementChanged)
        {
            return;
        }

        var outcome = broadcaster.SendHostStreamDemand(sessionId, transition.Current, reason);
        if (outcome == ChannelByteSendQueueWriteOutcome.Full)
        {
            metrics.RecordQueueOverflow(QueueOverflowLane.Host);
            logger.LogWarning(
                "Host queue full while sending stream demand update for session {SessionId}",
                sessionId);
        }
    }
}
