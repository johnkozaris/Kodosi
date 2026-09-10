using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class RoomMemberRepository(KodosiDbContext context)
    : IRoomMemberRepository, ISharedRoomAuthorizationRepository
{
    private readonly KodosiDbContext _context = context;

    public async Task<bool> IsMemberAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
        => await _context.RoomMembers
            .AnyAsync(wm => wm.RoomId == roomId && wm.UserId == userId && wm.RevokedAt == null, ct);

    public async Task<IReadOnlyList<RoomId>> GetActiveRoomIdsForUserAsync(
        UserId userId,
        IReadOnlyList<RoomId> roomIds,
        CancellationToken ct = default)
    {
        if (roomIds.Count == 0)
        {
            return [];
        }

        return await _context.RoomMembers
            .Where(wm => wm.UserId == userId && wm.RevokedAt == null && roomIds.Contains(wm.RoomId))
            .Select(wm => wm.RoomId)
            .Distinct()
            .ToListAsync(ct);
    }

    public async Task<RoomMember?> GetAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
        => await _context.RoomMembers
            .FirstOrDefaultAsync(wm => wm.RoomId == roomId && wm.UserId == userId, ct);

    public async Task AddAsync(RoomMember member, CancellationToken ct = default)
        => await _context.RoomMembers.AddAsync(member, ct);

    public async Task<IReadOnlyList<UserId>> GetMemberUserIdsAsync(
        RoomId roomId, CancellationToken ct = default)
        => await _context.RoomMembers
            .Where(wm => wm.RoomId == roomId && wm.RevokedAt == null)
            .Select(wm => wm.UserId)
            .ToListAsync(ct);

    public async Task<IReadOnlyList<RoomMember>> GetActiveByRoomAsync(
        RoomId roomId,
        CancellationToken ct = default)
        => await _context.RoomMembers
            .AsNoTracking()
            .Where(member => member.RoomId == roomId && member.RevokedAt == null)
            .OrderBy(member => member.UserId)
            .ToListAsync(ct);

    public async Task<IReadOnlyList<RoomMember>> GetAdmissionProofPageAfterAsync(
        RoomId roomId,
        Guid? afterUserId,
        int limit,
        CancellationToken ct = default)
    {
        var query = _context.RoomMembers
            .AsNoTracking()
            .Where(member =>
                member.RoomId == roomId
                && member.RevokedAt == null
                && member.Role != RoomRole.Owner);
        if (afterUserId is { } cursor)
        {
            query = query.Where(member => member.UserId.Value.CompareTo(cursor) > 0);
        }
        return await query
            .OrderBy(member => member.UserId)
            .Take(limit)
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<RoomId>> GetSharedActiveRoomIdsAsync(
        UserId firstUserId,
        UserId secondUserId,
        CancellationToken ct = default) =>
        await _context.RoomMembers
            .AsNoTracking()
            .Where(first =>
                first.UserId == firstUserId
                && first.RevokedAt == null
                && _context.RoomMembers.Any(second =>
                    second.RoomId == first.RoomId
                    && second.UserId == secondUserId
                    && second.RevokedAt == null))
            .Select(member => member.RoomId)
            .Distinct()
            .OrderBy(roomId => roomId)
            .ToListAsync(ct);

    public async Task<IReadOnlyList<RoomId>> GetHistoricalArtifactRoomIdsAsync(
        UserId readerUserId,
        UserId authorUserId,
        CancellationToken ct = default) =>
        await _context.RoomMembers
            .AsNoTracking()
            .Where(reader => reader.UserId == readerUserId
                && reader.RevokedAt == null
                && (_context.RoomChatMessages.Any(message =>
                        message.RoomId == reader.RoomId && message.AuthorUserId == authorUserId)
                    || _context.RoomTasks.Any(task => task.RoomId == reader.RoomId
                        && (task.CreatedByUserId == authorUserId
                            || (task.Result != null && task.ResultAuthorUserId == authorUserId)))))
            .Select(reader => reader.RoomId)
            .Distinct()
            .OrderBy(roomId => roomId)
            .ToListAsync(ct);

    public async Task<IReadOnlyList<UserId>> GetRoomPeerUserIdsAsync(
        UserId userId,
        CancellationToken ct = default)
    {


        return await _context.RoomMembers
            .Where(peer => peer.RevokedAt == null
                && peer.UserId != userId
                && _context.RoomMembers
                    .Any(me => me.RoomId == peer.RoomId
                        && me.UserId == userId
                        && me.RevokedAt == null))
            .Select(peer => peer.UserId)
            .Distinct()
            .ToListAsync(ct);
    }
}
