using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class InputAuditRepository(KodosiDbContext context) : IInputAuditRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task AddAsync(InputAuditEntry entry, CancellationToken ct = default)
        => await _context.InputAuditEntries.AddAsync(entry, ct);

    public async Task AddRangeAsync(IEnumerable<InputAuditEntry> entries, CancellationToken ct = default)
        => await _context.InputAuditEntries.AddRangeAsync(entries, ct);

    public async Task<InputAuditEntry?> GetByClientCommandAsync(
        SessionId sessionId,
        UserId senderUserId,
        string clientCommandId,
        CancellationToken ct = default)
        => await _context.InputAuditEntries.FirstOrDefaultAsync(
            entry => entry.SessionId == sessionId
                && entry.SenderUserId == senderUserId
                && entry.ClientCommandId == clientCommandId,
            ct);

    public async Task<IReadOnlySet<InputAuditLookupKey>> GetExistingClientCommandsAsync(
        IReadOnlyCollection<InputAuditLookupKey> keys,
        CancellationToken ct = default)
    {
        if (keys.Count == 0)
        {
            return new HashSet<InputAuditLookupKey>();
        }

        var existing = new HashSet<InputAuditLookupKey>();
        foreach (var group in keys.GroupBy(static key => (key.SessionId, key.SenderUserId)))
        {
            var clientCommandIds = group
                .Select(static key => key.ClientCommandId)
                .Distinct(StringComparer.Ordinal)
                .ToArray();

            var matches = await _context.InputAuditEntries
                .Where(entry =>
                    entry.SessionId == group.Key.SessionId &&
                    entry.SenderUserId == group.Key.SenderUserId &&
                    clientCommandIds.Contains(entry.ClientCommandId))
                .Select(entry => new InputAuditLookupKey(
                    entry.SessionId,
                    entry.SenderUserId,
                    entry.ClientCommandId))
                .ToListAsync(ct);

            foreach (var match in matches)
            {
                existing.Add(match);
            }
        }

        return existing;
    }

    public async Task<bool> TryRecordDuplicateAsync(
        SessionId sessionId,
        UserId senderUserId,
        string clientCommandId,
        DateTimeOffset occurredAt,
        CancellationToken ct = default)
    {
        var updated = await _context.InputAuditEntries
            .Where(entry =>
                entry.SessionId == sessionId
                && entry.SenderUserId == senderUserId
                && entry.ClientCommandId == clientCommandId)
            .ExecuteUpdateAsync(
                setters => setters
                    .SetProperty(
                        entry => entry.DuplicateCount,
                        entry => entry.DuplicateCount + 1)
                    .SetProperty(entry => entry.LastDuplicateAt, occurredAt),
                ct);
        return updated == 1;
    }
}
