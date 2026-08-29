using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class AccessOverrideAuditRepository(KodosiDbContext context) : IAccessOverrideAuditRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task AddAsync(AccessOverrideAuditEntry entry, CancellationToken ct = default)
    {
        await _context.Set<AccessOverrideAuditEntry>().AddAsync(entry, ct);
    }
}
