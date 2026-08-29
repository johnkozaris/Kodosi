using Kodosi.Domain;

namespace Kodosi.Application;

public interface IFriendshipRepository
{
    Task<bool> AreFriendsAsync(UserId userA, UserId userB, CancellationToken ct = default);
    Task<IReadOnlyList<UserId>> GetFriendIdsAsync(UserId userId, CancellationToken ct = default);
    Task<IReadOnlyList<Friendship>> GetPendingIncomingAsync(UserId userId, CancellationToken ct = default);
    Task<IReadOnlyList<Friendship>> GetPendingOutgoingAsync(UserId userId, CancellationToken ct = default);
    Task<Friendship?> GetAsync(UserId userA, UserId userB, CancellationToken ct = default);
    Task AddAsync(Friendship friendship, CancellationToken ct = default);
    void Remove(Friendship friendship);
}
