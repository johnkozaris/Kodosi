namespace Kodosi.Application;

public sealed record SessionCreationReceiptResponse(
    Guid SessionId,
    Guid CreateIdempotencyKey,
    Guid IncarnationId,
    long Generation,
    int ProtocolVersion);
