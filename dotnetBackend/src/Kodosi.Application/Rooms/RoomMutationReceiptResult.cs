using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record RoomMutationReceiptResult(
    bool IsDuplicate,
    RoomId RoomId,
    Guid EntityId,
    string Result,
    long? Revision,
    Guid? AssigneeSessionId,
    Guid? AssigneeSessionIncarnationId);

public sealed class RoomMutationReceiptTargetConflictException : ConflictException
{
    public RoomMutationReceiptTargetConflictException()
        : base(
            "The room mutation request identifier was already used for a different target.",
            "ROOM_MUTATION_TARGET_CONFLICT")
    { }
}
