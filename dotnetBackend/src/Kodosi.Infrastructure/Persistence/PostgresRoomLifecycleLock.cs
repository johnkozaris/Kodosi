using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence;

public sealed class PostgresRoomLifecycleLock(KodosiDbContext context)
    : IRoomLifecycleLock
{
    private static ReadOnlySpan<byte> LockDomain => "kodosi-room-lifecycle"u8;
    private readonly KodosiDbContext _context = context;

    public async Task AcquireAsync(RoomId roomId, CancellationToken ct = default)
    {
        if (_context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "Room lifecycle lock requires an active database transaction.");
        }

        var (key1, key2) = PostgresAdvisoryKey.Derive(LockDomain, roomId.Value);
        _ = await _context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT pg_advisory_xact_lock({key1}, {key2})",
            ct);
    }
}
