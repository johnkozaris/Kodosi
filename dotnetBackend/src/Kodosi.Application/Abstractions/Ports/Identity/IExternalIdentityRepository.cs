using Kodosi.Domain;

namespace Kodosi.Application;

public interface IExternalIdentityRepository
{
    Task<ExternalIdentity?> GetAsync(
        string provider,
        string issuer,
        string subject,
        CancellationToken ct = default);

    Task AddAsync(ExternalIdentity identity, CancellationToken ct = default);
}
