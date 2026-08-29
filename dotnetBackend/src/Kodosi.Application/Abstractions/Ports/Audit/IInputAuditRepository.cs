using Kodosi.Domain;

namespace Kodosi.Application;

public readonly record struct InputAuditLookupKey(
    SessionId SessionId,
    UserId SenderUserId,
    string ClientCommandId);

public interface IInputAuditRepository
{
    Task AddAsync(InputAuditEntry entry, CancellationToken ct = default);
    Task AddRangeAsync(IEnumerable<InputAuditEntry> entries, CancellationToken ct = default);
    Task<InputAuditEntry?> GetByClientCommandAsync(
        SessionId sessionId,
        UserId senderUserId,
        string clientCommandId,
        CancellationToken ct = default);
    Task<IReadOnlySet<InputAuditLookupKey>> GetExistingClientCommandsAsync(
        IReadOnlyCollection<InputAuditLookupKey> keys,
        CancellationToken ct = default);
    Task<bool> TryRecordDuplicateAsync(
        SessionId sessionId,
        UserId senderUserId,
        string clientCommandId,
        DateTimeOffset occurredAt,
        CancellationToken ct = default);
}
