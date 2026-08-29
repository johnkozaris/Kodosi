using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class SemanticRelayLifecycleRepository(KodosiDbContext context)
    : ISemanticRelayLifecycleRepository
{
    public Task DeleteForDevicesAsync(
        UserId userId,
        IReadOnlyCollection<string> deviceIds,
        CancellationToken ct = default)
    {
        if (deviceIds.Count == 0)
        {
            return Task.CompletedTask;
        }
        return context.SemanticRelayRequests
            .Where(request => request.RequesterUserId == userId
                && deviceIds.Contains(request.RequesterDeviceId))
            .ExecuteDeleteAsync(ct);
    }

    public Task DeleteForAccountAsync(UserId userId, CancellationToken ct = default) =>
        context.SemanticRelayRequests
            .Where(request => request.RequesterUserId == userId)
            .ExecuteDeleteAsync(ct);
}
