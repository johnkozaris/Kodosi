using Kodosi.Data;
using Kodosi.Security;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Friends;

/// A user name tells whether its account exists, so an account can try only a few names of
/// people that it has no friendship or request with: a burst, then one each interval. An
/// account that is refused in many intervals in a row is banned; a burst of refusals counts once.
public sealed class NameChecks(KodosiDbContext db, ConnectionDirectory connections, DeviceSessions deviceSessions, TimeProvider clock)
{
    public const int Burst = 20;
    public static readonly TimeSpan Interval = TimeSpan.FromMinutes(30);
    public const int RefusedIntervalsBeforeBan = 10;

    public async Task TakeAsync(Guid userId, CancellationToken ct)
    {
        var now = clock.GetUtcNow();
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var user = await db.LockUserAsync(userId, ct);
        await db.Entry(user).ReloadAsync(ct);
        var limit = now + Interval * Burst;
        var used = user.NameChecksUntil is { } at && at > now ? (at < limit ? at : limit) : now;
        var until = used + Interval;
        var allowed = until <= limit;
        user.NameChecksUntil = allowed ? until : used;
        if (!allowed && (user.NameChecksRefusedAt is not { } refused || now - refused >= Interval))
        {
            var continued = user.NameChecksRefusedAt is { } last && now - last < 2 * Interval;
            user.NameChecksRefusedIntervals = continued ? user.NameChecksRefusedIntervals + 1 : 1;
            user.NameChecksRefusedAt = now;
            if (user.NameChecksRefusedIntervals >= RefusedIntervalsBeforeBan)
                (user.BannedAt, user.NameChecksRefusedAt, user.NameChecksRefusedIntervals) = (now, null, 0);
        }
        await db.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        if (allowed) return;
        if (user.BannedAt is null) throw new ApiException(429, "You tried too many user names. You can try another one in 30 minutes.");
        deviceSessions.Remove(userId);
        foreach (var device in await db.Devices.Where(x => x.UserId == userId).Select(x => x.Id).ToListAsync(ct))
            connections.RemoveDevice(userId, device);
        throw ApiException.Banned();
    }
}
