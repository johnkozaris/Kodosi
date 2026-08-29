using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class RoomRepository(KodosiDbContext context) : IRoomRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task<Room?> GetByIdAsync(RoomId id, CancellationToken ct = default)
        => await _context.Rooms.FirstOrDefaultAsync(w => w.Id == id, ct);

    public async Task AddAsync(Room room, CancellationToken ct = default)
        => await _context.Rooms.AddAsync(room, ct);

    public async Task<IReadOnlyList<Room>> GetByMemberPageAsync(
        UserId userId,
        FeedCursor? cursor,
        int limit,
        CancellationToken ct = default)
    {
        IQueryable<Room> query;
        if (cursor is null)
        {
            query = _context.Rooms.Where(room => _context.RoomMembers.Any(member =>
                member.RoomId == room.Id
                && member.UserId == userId
                && member.RevokedAt == null));
        }
        else
        {
            query = _context.Rooms.FromSqlInterpolated($"""
                SELECT room.*, room.xmin
                FROM rooms AS room
                WHERE (
                    room.created_at < {cursor.StartedAt}
                    OR (room.created_at = {cursor.StartedAt} AND room.id < {cursor.Id})
                )
                AND EXISTS (
                    SELECT 1
                    FROM room_members AS member
                    WHERE member.room_id = room.id
                      AND member.user_id = {userId.Value}
                      AND member.revoked_at IS NULL
                )
                """);
        }
        return await query
            .AsNoTracking()
            .OrderByDescending(room => room.CreatedAt)
            .ThenByDescending(room => room.Id)
            .Take(limit)
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<Room>> GetByIdsAsync(
        IReadOnlyCollection<RoomId> ids,
        CancellationToken ct = default)
    {
        if (ids.Count == 0)
        {
            return [];
        }

        return await _context.Rooms
            .AsNoTracking()
            .Where(room => ids.Contains(room.Id))
            .ToListAsync(ct);
    }
}
