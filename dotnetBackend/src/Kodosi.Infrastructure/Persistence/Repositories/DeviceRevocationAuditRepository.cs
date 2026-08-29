using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class DeviceRevocationAuditRepository(KodosiDbContext context) : IDeviceRevocationAuditRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task AddAsync(DeviceRevocationAuditEntry entry, CancellationToken ct = default)
    {
        await _context.Set<DeviceRevocationAuditEntry>().AddAsync(entry, ct);
    }
}
