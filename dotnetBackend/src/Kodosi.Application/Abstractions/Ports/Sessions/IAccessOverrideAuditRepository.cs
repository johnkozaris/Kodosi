using Kodosi.Domain;

namespace Kodosi.Application;

public interface IAccessOverrideAuditRepository
{
    Task AddAsync(AccessOverrideAuditEntry entry, CancellationToken ct = default);
}
