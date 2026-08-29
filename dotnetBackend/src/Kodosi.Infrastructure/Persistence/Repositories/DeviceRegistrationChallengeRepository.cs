using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class DeviceRegistrationChallengeRepository(KodosiDbContext context) : IDeviceRegistrationChallengeRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task AddAsync(DeviceRegistrationChallenge challenge, CancellationToken ct = default)
        => await _context.DeviceRegistrationChallenges.AddAsync(challenge, ct);

    public async Task<DeviceRegistrationChallenge?> GetByIdAsync(Guid id, CancellationToken ct = default)
        => await _context.DeviceRegistrationChallenges.FirstOrDefaultAsync(c => c.Id == id, ct);

    public async Task<bool> TryConsumeAsync(
        DeviceRegistrationChallenge challenge,
        CancellationToken ct = default)
    {
        var now = DateTimeOffset.UtcNow;
        return await _context.DeviceRegistrationChallenges
            .Where(candidate => candidate.Id == challenge.Id
                && candidate.Version == challenge.Version
                && candidate.ExpiresAt > now)
            .ExecuteDeleteAsync(ct) == 1;
    }

    public async Task<int> DeleteExpiredAsync(
        DateTimeOffset now,
        int limit,
        CancellationToken ct = default)
    {
        var ids = await _context.DeviceRegistrationChallenges
            .Where(challenge => challenge.ExpiresAt <= now)
            .OrderBy(challenge => challenge.ExpiresAt)
            .Select(challenge => challenge.Id)
            .Take(Math.Clamp(limit, 1, 1_000))
            .ToListAsync(ct);
        if (ids.Count == 0)
        {
            return 0;
        }

        return await _context.DeviceRegistrationChallenges
            .Where(challenge => ids.Contains(challenge.Id))
            .ExecuteDeleteAsync(ct);
    }
}
