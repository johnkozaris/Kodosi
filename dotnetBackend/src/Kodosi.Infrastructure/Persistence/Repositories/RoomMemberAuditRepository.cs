using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class RoomMemberAuditRepository(KodosiDbContext context) : IRoomMemberAuditRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task AddAsync(RoomMemberAuditEntry entry, CancellationToken ct = default)
    {
        await _context.Set<RoomMemberAuditEntry>().AddAsync(entry, ct);
    }
}
