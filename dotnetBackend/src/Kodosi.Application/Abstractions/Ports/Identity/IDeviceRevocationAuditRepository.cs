using Kodosi.Domain;

namespace Kodosi.Application;

public interface IDeviceRevocationAuditRepository
{
    Task AddAsync(DeviceRevocationAuditEntry entry, CancellationToken ct = default);
}
