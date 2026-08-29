using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISessionAccessMutationRepository
{
    Task AcquireAsync(
        UserId requesterUserId,
        Guid mutationId,
        CancellationToken ct = default);

    Task<SessionAccessMutation?> GetAsync(
        UserId requesterUserId,
        Guid mutationId,
        CancellationToken ct = default);

    Task AddAsync(
        SessionAccessMutation mutation,
        CancellationToken ct = default);
}
