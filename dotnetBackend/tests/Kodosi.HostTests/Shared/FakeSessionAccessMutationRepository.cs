using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

internal sealed class FakeSessionAccessMutationRepository : ISessionAccessMutationRepository
{
    private readonly Dictionary<(UserId, Guid), SessionAccessMutation> _mutations = [];

    public Task AcquireAsync(
        UserId requesterUserId,
        Guid mutationId,
        CancellationToken ct = default) => Task.CompletedTask;

    public Task<SessionAccessMutation?> GetAsync(
        UserId requesterUserId,
        Guid mutationId,
        CancellationToken ct = default) =>
        Task.FromResult(_mutations.GetValueOrDefault((requesterUserId, mutationId)));

    public Task AddAsync(
        SessionAccessMutation mutation,
        CancellationToken ct = default)
    {
        _mutations[(mutation.RequesterUserId, mutation.MutationId)] = mutation;
        return Task.CompletedTask;
    }
}
