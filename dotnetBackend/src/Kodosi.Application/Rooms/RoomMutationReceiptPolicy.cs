using Kodosi.Domain;

namespace Kodosi.Application;

internal static class RoomMutationReceiptPolicy
{
    public static void EnsureOperation(
        RoomMutationReceipt receipt,
        RoomMutationOperation operation)
    {
        if (receipt.Operation != operation)
        {
            throw new RoomMutationReceiptTargetConflictException();
        }
    }

    public static RoomMutationReceiptResult ResolveDuplicate(
        RoomMutationReceipt receipt,
        RoomMutationTargetFingerprint fingerprint)
    {
        if (!receipt.MatchesFingerprint(fingerprint.Value))
        {
            throw new RoomMutationReceiptTargetConflictException();
        }

        return new RoomMutationReceiptResult(
            IsDuplicate: true,
            receipt.RoomId,
            receipt.EntityId,
            receipt.Result,
            receipt.Revision,
            receipt.AssigneeSessionId,
            receipt.AssigneeSessionIncarnationId);
    }

    public static RoomMutationReceipt Create(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        RoomMutationTargetFingerprint fingerprint,
        RoomId roomId,
        Guid entityId,
        string result,
        long? revision,
        Guid? assigneeSessionId,
        Guid? assigneeSessionIncarnationId,
        DateTimeOffset createdAt) =>
        RoomMutationReceipt.Create(
            actorUserId,
            operation,
            requestId,
            fingerprint.Value,
            roomId,
            entityId,
            result,
            assigneeSessionId,
            assigneeSessionIncarnationId,
            revision,
            createdAt);
}
