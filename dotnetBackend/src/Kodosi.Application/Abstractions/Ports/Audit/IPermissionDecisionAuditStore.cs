using Kodosi.Domain;

namespace Kodosi.Application;

public readonly record struct PermissionDecisionPendingTuple(
    Guid SessionIncarnationId,
    long SessionIncarnationGeneration,
    string RequestId,
    long RequestGeneration,
    string RequesterDeviceId);

public enum PermissionDecisionAdmissionOutcome
{
    Applied,
    PendingDuplicate,
    AcceptedDuplicate,
    RejectedDuplicate,
    Rearmed,
    Conflict,
    Failed,
}

public readonly record struct PermissionDecisionAdmissionResult(
    PermissionDecisionAdmissionOutcome Outcome,
    PermissionDecisionPendingTuple? CanonicalTuple = null);

public enum HostActionCompletionOutcome
{
    Applied,
    Duplicate,
    Conflict,
    NotFound,
}

public readonly record struct HostActionCompletionResult(
    HostActionCompletionOutcome Outcome,
    UserId RequesterUserId,
    string ActionId,
    PermissionDecisionPendingTuple? CanonicalTuple = null);

public interface IPermissionDecisionAuditStore
{
    Task<PermissionDecisionAdmissionResult> AdmitAsync(
        SessionId sessionId,
        UserId requesterUserId,
        string actionId,
        string auditPayload,
        PermissionDecisionPendingTuple pendingTuple,
        CancellationToken ct = default);

    Task<bool> MarkDispatchFailedAsync(
        SessionId sessionId,
        UserId requesterUserId,
        string actionId,
        PermissionDecisionPendingTuple pendingTuple,
        CancellationToken ct = default);

    Task<HostActionCompletionResult> CompleteHostActionAsync(
        SessionId sessionId,
        UserId assertedRequesterUserId,
        string assertedActionId,
        PermissionDecisionPendingTuple assertedTuple,
        InputAuditStatus terminalStatus,
        CancellationToken ct = default);
}
