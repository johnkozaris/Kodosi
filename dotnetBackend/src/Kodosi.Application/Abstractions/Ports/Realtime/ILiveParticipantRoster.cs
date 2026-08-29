namespace Kodosi.Application;

public interface ILiveParticipantRoster
{
    bool TryAddSharedParticipant(
        string connectionId,
        int maxSharedParticipantCount,
        out StreamDemandTransition transition);

    StreamDemandTransition RemoveSharedParticipant(string connectionId);
    bool TryRemoveSharedParticipantIfStale(
        string connectionId,
        TimeSpan timeout,
        out StreamDemandTransition transition);
    bool TryAddOwnerParticipant(
        string connectionId,
        int maxOwnerParticipantCount,
        out StreamDemandTransition transition);
    StreamDemandTransition RemoveOwnerParticipant(string connectionId);
    bool TryRemoveOwnerParticipantIfStale(
        string connectionId,
        TimeSpan timeout,
        out StreamDemandTransition transition);
    void RecordParticipantActivity(string connectionId);
    IReadOnlyList<string> GetStaleSharedParticipantConnectionIds(TimeSpan timeout);
    IReadOnlyList<string> GetStaleOwnerParticipantConnectionIds(TimeSpan timeout);
}
