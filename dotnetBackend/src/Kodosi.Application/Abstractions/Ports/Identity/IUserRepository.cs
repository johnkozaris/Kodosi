using Kodosi.Domain;

namespace Kodosi.Application;

public interface IUserRepository
{
    Task<User?> GetByIdAsync(UserId id, CancellationToken ct = default);
    Task<User?> GetByHandleAsync(string handle, CancellationToken ct = default);
    Task<IReadOnlyList<string>> GetHandlesByPrefixAsync(string prefix, CancellationToken ct = default);
    Task<IReadOnlyList<User>> GetByIdsAsync(IReadOnlyList<UserId> ids, CancellationToken ct = default);
    Task AddAsync(User user, CancellationToken ct = default);
}
