namespace Kodosi.Application;

public sealed record StreamDemandTransition(
    StreamDemandSnapshot Previous,
    StreamDemandSnapshot Current)
{
    public bool Changed =>
        Previous.SharedParticipantCount != Current.SharedParticipantCount
        || Previous.OwnerParticipantCount != Current.OwnerParticipantCount;

    public bool RequirementChanged => Previous.Required != Current.Required;
}
