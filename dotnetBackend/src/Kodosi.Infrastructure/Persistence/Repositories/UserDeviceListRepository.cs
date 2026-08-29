using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class UserDeviceListRepository(KodosiDbContext context) : IUserDeviceListRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task<UserDeviceList?> GetLatestAsync(UserId userId, CancellationToken ct = default)
        => await _context.UserDeviceLists
            .Where(l => l.UserId == userId)
            .OrderByDescending(l => l.Generation)
            .FirstOrDefaultAsync(ct);

    public async Task<UserDeviceList?> GetGenerationAsync(
        UserId userId,
        long generation,
        CancellationToken ct = default) =>
        await _context.UserDeviceLists
            .AsNoTracking()
            .SingleOrDefaultAsync(
                list => list.UserId == userId && list.Generation == generation,
                ct);

    public async Task<IReadOnlyList<UserDeviceList>> GetLatestByUserIdsAsync(
        IReadOnlyCollection<UserId> userIds,
        CancellationToken ct = default)
    {
        if (userIds.Count == 0)
        {
            return [];
        }
        return await _context.UserDeviceLists
            .AsNoTracking()
            .Where(list => userIds.Contains(list.UserId))
            .GroupBy(list => list.UserId)
            .Select(group => group.OrderByDescending(list => list.Generation).First())
            .ToListAsync(ct);
    }

    public async Task AddAsync(UserDeviceList list, CancellationToken ct = default)
        => await _context.UserDeviceLists.AddAsync(list, ct);

    public async Task<int> RemoveAllForUserAsync(UserId userId, CancellationToken ct = default)
    {


        var lists = await _context.UserDeviceLists
            .Where(l => l.UserId == userId)
            .ToListAsync(ct);
        _context.UserDeviceLists.RemoveRange(lists);
        return lists.Count;
    }
}
