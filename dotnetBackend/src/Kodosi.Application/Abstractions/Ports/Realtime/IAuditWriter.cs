using Kodosi.Domain;

namespace Kodosi.Application;

public enum AuditAppendOutcome
{
    Appended,
    AlreadyExists,
    Failed,
}

public interface IAuditWriter
{
    Task<AuditAppendOutcome> AppendAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        InputAuditKind kind,
        string payload,
        InputAuditStatus status,
        CancellationToken ct = default);

    Task<bool> RecordDuplicateAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        InputAuditKind kind,
        string payload,
        CancellationToken ct = default);

    Task<bool> UpdateStatusAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        InputAuditStatus status,
        CancellationToken ct = default);

    Task<bool> TryRearmAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        InputAuditStatus initialStatus,
        CancellationToken ct = default);

    Task<InputAuditStatus?> GetStatusAsync(
        SessionId sessionId,
        UserId userId,
        string clientCommandId,
        CancellationToken ct = default);
}
