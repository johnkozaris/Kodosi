using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class FriendshipAuditRepository(KodosiDbContext context) : IFriendshipAuditRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task AddAsync(FriendshipAuditEntry entry, CancellationToken ct = default)
    {
        await _context.Set<FriendshipAuditEntry>().AddAsync(entry, ct);
    }
}
