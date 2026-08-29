using Kodosi.Domain;

namespace Kodosi.Application;

public interface IActionDedupeCache
{
    ActionDedupeClaim Claim(
        SessionId sessionId,
        UserId userId,
        string actionId,
        string? requestId = null);

    void Complete(
        SessionId sessionId,
        UserId userId,
        string actionId,
        long leaseId,
        ActionDedupeFinalOutcome outcome);

    void CompleteCurrent(
        SessionId sessionId,
        UserId userId,
        string actionId,
        ActionDedupeFinalOutcome outcome);

    bool TryClaimAuditSlot(SessionId sessionId, UserId userId, string actionId);

    void Sweep();
}

public enum ActionDedupeClaimKind
{
    New,
    Duplicate,
    Saturated,
}

public enum ActionDedupeFinalOutcome
{
    Accepted,
    Duplicate,
    Busy,
    Rejected,
}

public readonly record struct ActionDedupeClaim(
    ActionDedupeClaimKind Kind,
    long LeaseId,
    Task<ActionDedupeFinalOutcome>? OriginalOutcome,
    string? OriginalRequestId);
