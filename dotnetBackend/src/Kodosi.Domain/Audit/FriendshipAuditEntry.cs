
namespace Kodosi.Domain;

public sealed class FriendshipAuditEntry
{
    public Guid Id { get; private set; }
    public UserId ActorUserId { get; private set; }
    public UserId OtherUserId { get; private set; }
    public FriendshipAuditAction Action { get; private set; }
    public DateTimeOffset OccurredAt { get; private set; }
    public string? ClientIp { get; private set; }
    public string? UserAgent { get; private set; }

    private FriendshipAuditEntry() { }

    public static FriendshipAuditEntry Create(
        UserId actorUserId,
        UserId otherUserId,
        FriendshipAuditAction action,
        string? clientIp,
        string? userAgent)
    {
        if (actorUserId == otherUserId)
        {
            throw new DomainException("Friendship audit entry cannot reference the same user on both sides.");
        }

        return new FriendshipAuditEntry
        {
            Id = Guid.NewGuid(),
            ActorUserId = actorUserId,
            OtherUserId = otherUserId,
            Action = action,
            OccurredAt = DateTimeOffset.UtcNow,
            ClientIp = clientIp,
            UserAgent = userAgent,
        };
    }
}

public enum FriendshipAuditAction
{
    RequestSent,
    RequestAccepted,
    RequestRejected,
    RequestCancelled,
    Removed,
}
