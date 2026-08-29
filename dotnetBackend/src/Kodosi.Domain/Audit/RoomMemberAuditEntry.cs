
namespace Kodosi.Domain;

public sealed class RoomMemberAuditEntry
{
    public Guid Id { get; private set; }
    public RoomId RoomId { get; private set; }
    public UserId ActorUserId { get; private set; }
    public UserId TargetUserId { get; private set; }
    public RoomMemberAuditAction Action { get; private set; }
    public DateTimeOffset OccurredAt { get; private set; }
    public string? ClientIp { get; private set; }
    public string? UserAgent { get; private set; }

    private RoomMemberAuditEntry() { }

    public static RoomMemberAuditEntry Create(
        RoomId roomId,
        UserId actorUserId,
        UserId targetUserId,
        RoomMemberAuditAction action,
        string? clientIp,
        string? userAgent)
    {
        return new RoomMemberAuditEntry
        {
            Id = Guid.NewGuid(),
            RoomId = roomId,
            ActorUserId = actorUserId,
            TargetUserId = targetUserId,
            Action = action,
            OccurredAt = DateTimeOffset.UtcNow,
            ClientIp = clientIp,
            UserAgent = userAgent,
        };
    }
}

public enum RoomMemberAuditAction
{
    Added,
    Removed,
}
