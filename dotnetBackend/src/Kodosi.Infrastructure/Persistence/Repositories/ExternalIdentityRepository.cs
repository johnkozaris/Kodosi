using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class ExternalIdentityRepository(KodosiDbContext context) : IExternalIdentityRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task<ExternalIdentity?> GetAsync(
        string provider,
        string issuer,
        string subject,
        CancellationToken ct = default)
    {
        var normalizedProvider = provider.Trim().ToLowerInvariant();
        var normalizedIssuer = issuer.Trim();
        var normalizedSubject = subject.Trim();

        return await _context.ExternalIdentities.FirstOrDefaultAsync(
            identity =>
                identity.Provider == normalizedProvider
                && identity.Issuer == normalizedIssuer
                && identity.Subject == normalizedSubject,
            ct);
    }

    public async Task AddAsync(ExternalIdentity identity, CancellationToken ct = default)
        => await _context.ExternalIdentities.AddAsync(identity, ct);
}
