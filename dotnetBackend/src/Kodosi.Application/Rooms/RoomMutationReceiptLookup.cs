using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record RoomMutationReceiptSnapshot(
    Guid RequestId,
    string Operation,
    RoomId RoomId,
    Guid EntityId,
    string Result,
    long? Revision,
    Guid? AssigneeSessionId,
    Guid? AssigneeSessionIncarnationId,
    IReadOnlyList<byte> TargetFingerprint,
    DateTimeOffset CreatedAt);

public sealed class RoomMutationReceiptLookup(IRoomMutationReceiptRepository receipts)
{
    public async Task<RoomMutationReceiptSnapshot?> FindAsync(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        CancellationToken ct = default)
    {
        var receipt = await receipts.FindAsync(actorUserId, operation, requestId, ct);
        return receipt is null
            ? null
            : new RoomMutationReceiptSnapshot(
                receipt.RequestId,
                RoomMutationOperationWire.ToWireOperation(receipt.Operation),
                receipt.RoomId,
                receipt.EntityId,
                receipt.Result,
                receipt.Revision,
                receipt.AssigneeSessionId,
                receipt.AssigneeSessionIncarnationId,
                receipt.TargetFingerprint.ToArray(),
                receipt.CreatedAt);
    }

    public static RoomMutationOperation ParseWireOperation(string operation)
    {
        if (RoomMutationOperationWire.TryParseWireOperation(operation, out var parsed))
        {
            return parsed;
        }
        throw new InvalidParameterException(
            "operation",
            "must identify a supported room mutation");
    }
}
