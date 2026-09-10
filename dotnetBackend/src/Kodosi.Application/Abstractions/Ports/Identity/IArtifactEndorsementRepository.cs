using Kodosi.Domain;

namespace Kodosi.Application;

public interface IArtifactEndorsementRepository
{
    Task<ArtifactEndorsement?> GetAsync(UserId userId, Guid incarnationId, string digest, CancellationToken ct = default);
    Task<IReadOnlyList<ArtifactEndorsement>> ListAsync(UserId userId, Guid incarnationId, int offset, int limit, CancellationToken ct = default);
    Task<int> CountAsync(UserId userId, Guid incarnationId, CancellationToken ct = default);
    Task DeleteOtherIncarnationsAsync(UserId userId, Guid incarnationId, CancellationToken ct = default);
    Task AddAsync(ArtifactEndorsement endorsement, CancellationToken ct = default);
}
