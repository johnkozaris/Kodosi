using Kodosi.Domain;

namespace Kodosi.Application;

public enum SemanticRequestClaimKind
{
    Created,
    ExactDuplicate,
    Conflict,
}

public sealed record SemanticRequestClaim(
    SemanticRequestClaimKind Kind,
    SemanticRelayRequest Request);

public interface ISemanticRelayRepository
{
    Task<SemanticRequestClaim> ClaimRequestAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        CancellationToken ct = default);

    Task<SemanticRequestClaim?> FindExactRequestAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        CancellationToken ct = default);

    Task MarkDispatchedAsync(Guid requestRowId, CancellationToken ct = default);

    Task<bool> StoreReceiptAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        string mode,
        string payloadSha256,
        string outcome,
        UserId ownerUserId,
        string ownerDeviceId,
        string signature,
        CancellationToken ct = default);

    Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsForDeviceAsync(
        UserId requesterUserId,
        string requesterDeviceId,
        int limit,
        SemanticReceiptCursor? cursor = null,
        CancellationToken ct = default);

    Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        int limit,
        CancellationToken ct = default);

    Task<bool> AcknowledgeReceiptAsync(
        SessionId sessionId,
        Guid incarnationId,
        UserId requesterUserId,
        string requesterDeviceId,
        Guid requestId,
        CancellationToken ct = default);

    Task<int> DeleteAcknowledgedBeforeAsync(
        DateTimeOffset cutoff,
        int limit,
        CancellationToken ct = default);
}
