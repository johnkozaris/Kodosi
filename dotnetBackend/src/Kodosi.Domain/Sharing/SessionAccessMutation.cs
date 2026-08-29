namespace Kodosi.Domain;

public enum SessionAccessMutationKind
{
    Grant,
    Revoke,
    Leave,
}

public sealed class SessionAccessMutation
{
    private SessionAccessMutation() { }

    public UserId RequesterUserId { get; private set; }
    public Guid MutationId { get; private set; }
    public SessionId SessionId { get; private set; }
    public Guid IncarnationId { get; private set; }
    public SessionAccessMutationKind Kind { get; private set; }
    public UserId? TargetUserId { get; private set; }
    public AccessLevel? AccessLevel { get; private set; }
    public DateTimeOffset? RequestedExpiresAt { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }

    public static SessionAccessMutation Create(
        UserId requesterUserId,
        Guid mutationId,
        SessionId sessionId,
        Guid incarnationId,
        SessionAccessMutationKind kind,
        UserId? targetUserId,
        AccessLevel? accessLevel,
        DateTimeOffset? requestedExpiresAt,
        DateTimeOffset createdAt)
    {
        if (mutationId == Guid.Empty || mutationId.Version != 7)
        {
            throw new DomainException("Session access mutation ID must be a UUIDv7.");
        }
        if (incarnationId == Guid.Empty)
        {
            throw new DomainException("Session access mutation incarnation ID is required.");
        }
        var shapeValid = kind switch
        {
            SessionAccessMutationKind.Grant => targetUserId is not null
                && accessLevel is not null
                && requestedExpiresAt is not null,
            SessionAccessMutationKind.Revoke => targetUserId is not null
                && accessLevel is null
                && requestedExpiresAt is null,
            SessionAccessMutationKind.Leave => targetUserId is null
                && accessLevel is null
                && requestedExpiresAt is null,
            _ => false,
        };
        if (!shapeValid)
        {
            throw new DomainException("Session access mutation fields do not match its kind.");
        }
        return new SessionAccessMutation
        {
            RequesterUserId = requesterUserId,
            MutationId = mutationId,
            SessionId = sessionId,
            IncarnationId = incarnationId,
            Kind = kind,
            TargetUserId = targetUserId,
            AccessLevel = accessLevel,
            RequestedExpiresAt = requestedExpiresAt,
            CreatedAt = createdAt,
        };
    }

    public bool Matches(
        SessionId sessionId,
        Guid incarnationId,
        SessionAccessMutationKind kind,
        UserId? targetUserId,
        AccessLevel? accessLevel,
        DateTimeOffset? requestedExpiresAt) =>
        SessionId == sessionId
        && IncarnationId == incarnationId
        && Kind == kind
        && TargetUserId == targetUserId
        && AccessLevel == accessLevel
        && RequestedExpiresAt == requestedExpiresAt;
}
