using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class IdentityResetAuditRepository(KodosiDbContext context) : IIdentityResetAuditRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task AddAsync(IdentityResetAuditEntry entry, CancellationToken ct = default)
    {
        await _context.Set<IdentityResetAuditEntry>().AddAsync(entry, ct);
    }
}
