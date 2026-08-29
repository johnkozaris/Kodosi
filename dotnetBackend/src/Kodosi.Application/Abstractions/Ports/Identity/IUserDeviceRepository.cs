using Kodosi.Domain;

namespace Kodosi.Application;

public interface IUserDeviceRepository
{
    Task<IReadOnlyList<UserDevice>> GetByUserIdAsync(UserId userId, CancellationToken ct = default);
    Task<UserDevice?> GetByDeviceIdAsync(string deviceId, CancellationToken ct = default);
    Task<IReadOnlyDictionary<string, UserId>> GetUserIdsByDeviceIdsAsync(
        IReadOnlyCollection<string> deviceIds,
        CancellationToken ct = default);
    Task<IReadOnlyList<UserDevice>> GetByUserIdsAsync(IReadOnlyList<UserId> userIds, CancellationToken ct = default);
    Task AddAsync(UserDevice device, CancellationToken ct = default);
    Task<int> RemoveAllForUserAsync(UserId userId, CancellationToken ct = default);
    void Update(UserDevice device);
}
