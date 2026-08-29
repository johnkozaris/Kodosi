using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class FriendshipRepository(KodosiDbContext context) : IFriendshipRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task<bool> AreFriendsAsync(UserId userA, UserId userB, CancellationToken ct = default)
    {
        var (low, high) = OrderPair(userA, userB);
        return await _context.Friendships
            .AnyAsync(f => f.UserLowId == low && f.UserHighId == high
                && f.Status == FriendshipStatus.Accepted, ct);
    }

    public async Task<IReadOnlyList<UserId>> GetFriendIdsAsync(UserId userId, CancellationToken ct = default)
    {
        var asLow = await _context.Friendships
            .Where(f => f.UserLowId == userId && f.Status == FriendshipStatus.Accepted)
            .Select(f => f.UserHighId)
            .ToListAsync(ct);

        var asHigh = await _context.Friendships
            .Where(f => f.UserHighId == userId && f.Status == FriendshipStatus.Accepted)
            .Select(f => f.UserLowId)
            .ToListAsync(ct);

        return asLow.Concat(asHigh).ToList();
    }

    public async Task<IReadOnlyList<Friendship>> GetPendingIncomingAsync(UserId userId, CancellationToken ct = default)
    {
        return await _context.Friendships
            .Where(f => f.Status == FriendshipStatus.Pending
                && (f.UserLowId == userId || f.UserHighId == userId)
                && f.RequestorUserId != userId)
            .OrderBy(f => f.CreatedAt)
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<Friendship>> GetPendingOutgoingAsync(UserId userId, CancellationToken ct = default)
    {
        return await _context.Friendships
            .Where(f => f.Status == FriendshipStatus.Pending && f.RequestorUserId == userId)
            .OrderBy(f => f.CreatedAt)
            .ToListAsync(ct);
    }

    public async Task<Friendship?> GetAsync(UserId userA, UserId userB, CancellationToken ct = default)
    {
        var (low, high) = OrderPair(userA, userB);
        return await _context.Friendships
            .FirstOrDefaultAsync(f => f.UserLowId == low && f.UserHighId == high, ct);
    }

    public async Task AddAsync(Friendship friendship, CancellationToken ct = default)
        => await _context.Friendships.AddAsync(friendship, ct);

    public void Remove(Friendship friendship)
        => _context.Friendships.Remove(friendship);

    private static (UserId Low, UserId High) OrderPair(UserId a, UserId b)
        => a.Value.CompareTo(b.Value) <= 0 ? (a, b) : (b, a);
}
