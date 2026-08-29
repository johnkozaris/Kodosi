namespace Kodosi.Domain;

public sealed class RoomMutationSessionEffect
{
    private RoomMutationSessionEffect() { }

    private RoomMutationSessionEffect(
        UserId actorUserId,
        Guid requestId,
        SessionId sessionId,
        Guid sessionIncarnationId,
        UserId ownerUserId,
        DateTimeOffset startedAt,
        bool endedByRemoval)
    {
        ActorUserId = actorUserId;
        Operation = RoomMutationOperation.RemoveMember;
        RequestId = requestId;
        SessionId = sessionId;
        SessionIncarnationId = sessionIncarnationId;
        OwnerUserId = ownerUserId;
        StartedAt = startedAt;
        EndedByRemoval = endedByRemoval;
    }

    public UserId ActorUserId { get; private set; }
    public RoomMutationOperation Operation { get; private set; }
    public Guid RequestId { get; private set; }
    public SessionId SessionId { get; private set; }
    public Guid SessionIncarnationId { get; private set; }
    public UserId OwnerUserId { get; private set; }
    public DateTimeOffset StartedAt { get; private set; }
    public bool EndedByRemoval { get; private set; }

    public static RoomMutationSessionEffect Create(
        UserId actorUserId,
        Guid requestId,
        SessionId sessionId,
        Guid sessionIncarnationId,
        UserId ownerUserId,
        DateTimeOffset startedAt,
        bool endedByRemoval)
    {
        if (requestId == Guid.Empty || requestId.Version != 7)
        {
            throw new DomainException("Room mutation effect request ID must be a UUIDv7.");
        }
        if (sessionIncarnationId == Guid.Empty)
        {
            throw new DomainException("Room mutation effect session incarnation is required.");
        }
        return new RoomMutationSessionEffect(
            actorUserId,
            requestId,
            sessionId,
            sessionIncarnationId,
            ownerUserId,
            startedAt,
            endedByRemoval);
    }
}
