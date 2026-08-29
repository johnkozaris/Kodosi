using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class HostConnectionState(
    string connectionId,
    string sessionIdText,
    UserId authenticatedUserId,
    TimeProvider? timeProvider = null) : DeviceAuthorizedConnectionState(
        connectionId,
        sessionIdText,
        authenticatedUserId,
        timeProvider)
{
    public byte[]? DeviceProofChallenge { get; set; }
    public SessionSendQueues? SessionQueues { get; set; }
    public ChannelByteSendQueue? HostQueue { get; set; }
    public LiveSessionCreationOwnership? RuntimeCreationOwnership { get; set; }
    public bool DurableSlotReleaseRequired { get; set; }
    public bool HostClaimed { get; set; }
    public bool HostRegistered { get; set; }
    public bool HostAccepted { get; set; }
    public bool PersistedLiveBeforeAcceptance { get; set; }
    public DateTimeOffset? ActivatedSessionStartedAt { get; set; }
    public Guid? ActivatedSessionIncarnationId { get; set; }
    public long? ActivatedSessionIncarnationGeneration { get; set; }
    public Guid? ActivatedRuntimeIncarnationId { get; set; }
    public bool DiscardPreAcceptedState { get; set; }
    public bool HostEnded { get; set; }
    public CloseReason? CloseReason { get; set; }
}
