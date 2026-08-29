namespace Kodosi.Domain;

public sealed class SessionEndMutation
{
    private SessionEndMutation() { }

    private SessionEndMutation(
        UserId ownerUserId,
        Guid mutationId,
        SessionId sessionId,
        Guid incarnationId,
        Guid firstAttemptId,
        DateTimeOffset createdAt)
    {
        OwnerUserId = ownerUserId;
        MutationId = mutationId;
        SessionId = sessionId;
        IncarnationId = incarnationId;
        FirstAttemptId = firstAttemptId;
        CreatedAt = createdAt;
    }

    public UserId OwnerUserId { get; private set; }
    public Guid MutationId { get; private set; }
    public SessionId SessionId { get; private set; }
    public Guid IncarnationId { get; private set; }
    public Guid FirstAttemptId { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }

    public static SessionEndMutation Create(
        UserId ownerUserId,
        Guid mutationId,
        SessionId sessionId,
        Guid incarnationId,
        Guid firstAttemptId,
        DateTimeOffset createdAt)
    {
        if (mutationId == Guid.Empty || mutationId.Version != 7)
        {
            throw new DomainException("Session end mutation ID must be a UUIDv7.");
        }
        if (incarnationId == Guid.Empty)
        {
            throw new DomainException("Session incarnation ID is required.");
        }
        if (firstAttemptId == Guid.Empty || firstAttemptId.Version != 7)
        {
            throw new DomainException("Session end attempt ID must be a UUIDv7.");
        }
        return new SessionEndMutation(
            ownerUserId,
            mutationId,
            sessionId,
            incarnationId,
            firstAttemptId,
            createdAt);
    }

    public bool Matches(SessionId sessionId, Guid incarnationId) =>
        SessionId == sessionId && IncarnationId == incarnationId;
}
