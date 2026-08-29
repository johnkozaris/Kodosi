using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISemanticRelayLifecycleRepository
{
    Task DeleteForAccountAsync(UserId userId, CancellationToken ct = default);

    Task DeleteForDevicesAsync(
        UserId userId,
        IReadOnlyCollection<string> deviceIds,
        CancellationToken ct = default);
}
