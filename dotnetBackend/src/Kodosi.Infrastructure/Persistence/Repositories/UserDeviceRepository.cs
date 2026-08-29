using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class UserDeviceRepository(KodosiDbContext context) : IUserDeviceRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task<IReadOnlyList<UserDevice>> GetByUserIdAsync(
        UserId userId, CancellationToken ct = default)
        => await _context.UserDevices
            .Where(d => d.UserId == userId)
            .OrderByDescending(d => d.CreatedAt)
            .ToListAsync(ct);

    public async Task<UserDevice?> GetByDeviceIdAsync(
        string deviceId, CancellationToken ct = default)
        => await _context.UserDevices.FirstOrDefaultAsync(d => d.DeviceId == deviceId, ct);

    public async Task<IReadOnlyDictionary<string, UserId>>
        GetUserIdsByDeviceIdsAsync(
            IReadOnlyCollection<string> deviceIds,
            CancellationToken ct = default)
    {
        var distinctDeviceIds = deviceIds
            .Distinct(StringComparer.Ordinal)
            .ToList();
        return await _context.UserDevices
            .AsNoTracking()
            .Where(device => distinctDeviceIds.Contains(device.DeviceId))
            .ToDictionaryAsync(
                device => device.DeviceId,
                device => device.UserId,
                StringComparer.Ordinal,
                ct);
    }

    public async Task<IReadOnlyList<UserDevice>> GetByUserIdsAsync(
        IReadOnlyList<UserId> userIds, CancellationToken ct = default)
        => await _context.UserDevices
            .Where(d => userIds.Contains(d.UserId))
            .ToListAsync(ct);

    public async Task AddAsync(UserDevice device, CancellationToken ct = default)
        => await _context.UserDevices.AddAsync(device, ct);

    public void Update(UserDevice device)
    {
        _context.UserDevices.Update(device);
    }

    public async Task<int> RemoveAllForUserAsync(UserId userId, CancellationToken ct = default)
    {


        var devices = await _context.UserDevices
            .Where(d => d.UserId == userId)
            .ToListAsync(ct);
        _context.UserDevices.RemoveRange(devices);
        return devices.Count;
    }
}
