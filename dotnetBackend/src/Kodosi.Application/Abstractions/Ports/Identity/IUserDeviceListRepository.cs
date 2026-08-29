using Kodosi.Domain;

namespace Kodosi.Application;

public interface IUserDeviceListRepository
{
    Task<UserDeviceList?> GetLatestAsync(UserId userId, CancellationToken ct = default);

    Task<UserDeviceList?> GetGenerationAsync(
        UserId userId,
        long generation,
        CancellationToken ct = default);

    Task<IReadOnlyList<UserDeviceList>> GetLatestByUserIdsAsync(
        IReadOnlyCollection<UserId> userIds,
        CancellationToken ct = default);

    Task AddAsync(UserDeviceList list, CancellationToken ct = default);

    Task<int> RemoveAllForUserAsync(UserId userId, CancellationToken ct = default);
}
