using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class ArtifactEndorsementRepository(KodosiDbContext context) : IArtifactEndorsementRepository
{
    private IQueryable<ArtifactEndorsement> Current(UserId userId, Guid incarnationId) =>
        context.ArtifactEndorsements.Where(row => row.UserId == userId
            && row.IdentityIncarnationId == incarnationId
            && context.Users.Any(user => user.Id == userId && user.IdentityIncarnationId == incarnationId));

    public Task<ArtifactEndorsement?> GetAsync(UserId userId, Guid incarnationId, string digest, CancellationToken ct = default) =>
        Current(userId, incarnationId).SingleOrDefaultAsync(row => row.ArtifactDigest == digest, ct);

    public async Task<IReadOnlyList<ArtifactEndorsement>> ListAsync(UserId userId, Guid incarnationId, int offset, int limit, CancellationToken ct = default) =>
        await Current(userId, incarnationId).AsNoTracking().OrderBy(row => row.ArtifactDigest).Skip(offset).Take(limit).ToListAsync(ct);

    public Task<int> CountAsync(UserId userId, Guid incarnationId, CancellationToken ct = default) =>
        Current(userId, incarnationId).CountAsync(ct);

    public async Task DeleteOtherIncarnationsAsync(UserId userId, Guid incarnationId, CancellationToken ct = default) =>
        _ = await context.ArtifactEndorsements.Where(row => row.UserId == userId && row.IdentityIncarnationId != incarnationId).ExecuteDeleteAsync(ct);

    public async Task AddAsync(ArtifactEndorsement endorsement, CancellationToken ct = default) =>
        await context.ArtifactEndorsements.AddAsync(endorsement, ct);
}
