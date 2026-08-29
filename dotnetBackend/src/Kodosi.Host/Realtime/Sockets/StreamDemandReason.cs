namespace Kodosi.Host.Realtime;

public enum StreamDemandReason
{
    HostConnected,
    OwnerParticipantJoined,
    OwnerParticipantLeft,
    OwnerParticipantTimeout,
    SharedParticipantJoined,
    SharedParticipantLeft,
    SharedParticipantTimeout,
}

internal static class StreamDemandReasonWire
{
    public static string ToWire(this StreamDemandReason reason) =>
        reason switch
        {
            StreamDemandReason.HostConnected => "host_connected",
            StreamDemandReason.OwnerParticipantJoined => "owner_participant_joined",
            StreamDemandReason.OwnerParticipantLeft => "owner_participant_left",
            StreamDemandReason.OwnerParticipantTimeout => "owner_participant_timeout",
            StreamDemandReason.SharedParticipantJoined => "shared_participant_joined",
            StreamDemandReason.SharedParticipantLeft => "shared_participant_left",
            StreamDemandReason.SharedParticipantTimeout => "shared_participant_timeout",
            _ => throw new ArgumentOutOfRangeException(nameof(reason), reason, "Unknown stream-demand reason."),
        };
}
