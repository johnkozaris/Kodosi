using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISessionEndMutationRepository
{
    Task AcquireAsync(
        UserId ownerUserId,
        Guid mutationId,
        CancellationToken ct = default);

    Task<SessionEndMutation?> GetAsync(
        UserId ownerUserId,
        Guid mutationId,
        CancellationToken ct = default);

    Task AddAsync(
        SessionEndMutation mutation,
        CancellationToken ct = default);
}
