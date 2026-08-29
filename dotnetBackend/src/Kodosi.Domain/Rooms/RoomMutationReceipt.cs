using System.Security.Cryptography;

namespace Kodosi.Domain;

public enum RoomMutationOperation
{
    AcceptInvitation,
    DeclineInvitation,
    CancelInvitation,
    RemoveMember,
    AssignTask,
    TransitionTask,
}

public static class RoomMutationOperationWire
{
    private static readonly IReadOnlyDictionary<string, RoomMutationOperation> ByToken =
        new Dictionary<string, RoomMutationOperation>(StringComparer.Ordinal)
        {
            ["acceptInvitation"] = RoomMutationOperation.AcceptInvitation,
            ["declineInvitation"] = RoomMutationOperation.DeclineInvitation,
            ["cancelInvitation"] = RoomMutationOperation.CancelInvitation,
            ["removeMember"] = RoomMutationOperation.RemoveMember,
            ["tasks.assign"] = RoomMutationOperation.AssignTask,
            ["tasks.transition"] = RoomMutationOperation.TransitionTask,
        };

    public static bool TryParseWireOperation(
        string token,
        out RoomMutationOperation operation) =>
        ByToken.TryGetValue(token, out operation);

    public static string ToWireOperation(RoomMutationOperation operation) =>
        operation switch
        {
            RoomMutationOperation.AcceptInvitation => "acceptInvitation",
            RoomMutationOperation.DeclineInvitation => "declineInvitation",
            RoomMutationOperation.CancelInvitation => "cancelInvitation",
            RoomMutationOperation.RemoveMember => "removeMember",
            RoomMutationOperation.AssignTask => "tasks.assign",
            RoomMutationOperation.TransitionTask => "tasks.transition",
            _ => throw new InvalidOperationException(
                $"Unknown room mutation operation: {operation}."),
        };
}

public sealed class RoomMutationReceipt
{
    public const int FingerprintLength = 32;

    private RoomMutationReceipt() { }

    private RoomMutationReceipt(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        byte[] targetFingerprint,
        RoomId roomId,
        Guid entityId,
        string result,
        Guid? assigneeSessionId,
        Guid? assigneeSessionIncarnationId,
        long? revision,
        DateTimeOffset createdAt)
    {
        ActorUserId = actorUserId;
        Operation = operation;
        RequestId = requestId;
        TargetFingerprint = targetFingerprint;
        RoomId = roomId;
        EntityId = entityId;
        Result = result;
        AssigneeSessionId = assigneeSessionId;
        AssigneeSessionIncarnationId = assigneeSessionIncarnationId;
        Revision = revision;
        CreatedAt = createdAt;
    }

    public UserId ActorUserId { get; private set; }
    public RoomMutationOperation Operation { get; private set; }
    public Guid RequestId { get; private set; }
    public byte[] TargetFingerprint { get; private set; } = [];
    public RoomId RoomId { get; private set; }
    public Guid EntityId { get; private set; }
    public string Result { get; private set; } = string.Empty;
    public Guid? AssigneeSessionId { get; private set; }
    public Guid? AssigneeSessionIncarnationId { get; private set; }
    public long? Revision { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }

    public static RoomMutationReceipt Create(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        ReadOnlySpan<byte> targetFingerprint,
        RoomId roomId,
        Guid entityId,
        string result,
        Guid? assigneeSessionId,
        Guid? assigneeSessionIncarnationId,
        long? revision,
        DateTimeOffset createdAt)
    {
        if (!Enum.IsDefined(operation))
        {
            throw new DomainException("Room mutation operation is invalid.");
        }
        if (requestId == Guid.Empty || requestId.Version != 7)
        {
            throw new DomainException("Room mutation request ID must be a UUIDv7.");
        }
        if (targetFingerprint.Length != FingerprintLength)
        {
            throw new DomainException("Room mutation target fingerprint must be SHA-256.");
        }
        if (entityId == Guid.Empty)
        {
            throw new DomainException("Room mutation result entity ID is required.");
        }
        if (string.IsNullOrWhiteSpace(result) || result.Length > 32)
        {
            throw new DomainException("Room mutation result is invalid.");
        }
        if (assigneeSessionId.HasValue != assigneeSessionIncarnationId.HasValue)
        {
            throw new DomainException(
                "Room mutation assignee session and incarnation must be present together.");
        }
        if (revision is < 0)
        {
            throw new DomainException("Room mutation result revision cannot be negative.");
        }

        return new RoomMutationReceipt(
            actorUserId,
            operation,
            requestId,
            targetFingerprint.ToArray(),
            roomId,
            entityId,
            result,
            assigneeSessionId,
            assigneeSessionIncarnationId,
            revision,
            createdAt);
    }

    public bool MatchesFingerprint(ReadOnlySpan<byte> fingerprint) =>
        fingerprint.Length == FingerprintLength
        && CryptographicOperations.FixedTimeEquals(TargetFingerprint, fingerprint);
}
