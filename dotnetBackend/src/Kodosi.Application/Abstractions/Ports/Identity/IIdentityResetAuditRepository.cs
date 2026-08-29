using Kodosi.Domain;

namespace Kodosi.Application;

public interface IIdentityResetAuditRepository
{
    Task AddAsync(IdentityResetAuditEntry entry, CancellationToken ct = default);
}
