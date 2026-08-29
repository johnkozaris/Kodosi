namespace Kodosi.Application;

public sealed record StreamDemandSnapshot(
    int SharedParticipantCount,
    int OwnerParticipantCount)
{
    public int ParticipantCount => SharedParticipantCount + OwnerParticipantCount;
    public bool Required => ParticipantCount > 0;
}
