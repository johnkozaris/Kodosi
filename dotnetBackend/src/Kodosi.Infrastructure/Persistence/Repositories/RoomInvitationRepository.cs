using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class RoomInvitationRepository(
    KodosiDbContext context,
    TimeProvider timeProvider) : IRoomInvitationRepository
{
    private readonly KodosiDbContext _context = context;
    private readonly TimeProvider _timeProvider = timeProvider;

    public Task<RoomInvitation?> GetByIdAsync(Guid id, CancellationToken ct = default)
        => _context.RoomInvitations.FirstOrDefaultAsync(i => i.Id == id, ct);

    public async Task<IReadOnlyList<RoomInvitation>> GetByIdsAsync(
        IReadOnlyCollection<Guid> ids,
        CancellationToken ct = default)
    {
        if (ids.Count == 0)
        {
            return [];
        }
        return await _context.RoomInvitations
            .Where(invitation => ids.Contains(invitation.Id))
            .ToListAsync(ct);
    }

    public Task<RoomInvitationLockUsers?> GetLockUsersAsync(
        Guid id,
        CancellationToken ct = default)
        => _context.RoomInvitations
            .AsNoTracking()
            .Where(invitation => invitation.Id == id)
            .Select(invitation => (RoomInvitationLockUsers?)new RoomInvitationLockUsers(
                invitation.RoomId,
                invitation.InvitedByUserId,
                invitation.InviteeUserId))
            .FirstOrDefaultAsync(ct);

    public Task<RoomInvitation?> GetPendingAsync(
        RoomId roomId,
        UserId inviteeUserId,
        CancellationToken ct = default)
        => _context.RoomInvitations
            .Where(i => i.RoomId == roomId
                && i.InviteeUserId == inviteeUserId
                && i.Status == RoomInvitationStatus.Pending)
            .FirstOrDefaultAsync(ct);

    public async Task<IReadOnlyList<RoomInvitation>> GetIncomingAsync(
        UserId userId,
        int limit,
        CancellationToken ct = default)
    {
        var now = _timeProvider.GetUtcNow();
        return await _context.RoomInvitations
            .AsNoTracking()
            .Where(i => i.InviteeUserId == userId
                && i.Status == RoomInvitationStatus.Pending
                && i.ExpiresAt > now
                && _context.Rooms.Any(room =>
                    room.Id == i.RoomId
                    && room.RosterGeneration == i.BaseRosterGeneration))
            .OrderByDescending(i => i.CreatedAt)
            .Take(Math.Clamp(limit, 1, 100))
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<RoomInvitation>> GetOutgoingAsync(
        UserId userId,
        int limit,
        CancellationToken ct = default)
    {
        var now = _timeProvider.GetUtcNow();
        return await _context.RoomInvitations
            .AsNoTracking()
            .Where(i => i.InvitedByUserId == userId
                && i.Status == RoomInvitationStatus.Pending
                && i.ExpiresAt > now
                && _context.Rooms.Any(room =>
                    room.Id == i.RoomId
                    && room.RosterGeneration == i.BaseRosterGeneration))
            .OrderByDescending(i => i.CreatedAt)
            .Take(Math.Clamp(limit, 1, 100))
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<RoomInvitation>> GetPendingByRoomAsync(
        RoomId roomId,
        CancellationToken ct = default)
        => await _context.RoomInvitations
            .Where(invitation =>
                invitation.RoomId == roomId
                && invitation.Status == RoomInvitationStatus.Pending)
            .ToListAsync(ct);

    public async Task<IReadOnlyList<RoomInvitation>> GetExpiredPendingAsync(
        DateTimeOffset now,
        int limit,
        CancellationToken ct = default)
        => await _context.RoomInvitations
            .AsNoTracking()
            .Where(invitation =>
                invitation.Status == RoomInvitationStatus.Pending
                && invitation.ExpiresAt <= now)
            .OrderBy(invitation => invitation.ExpiresAt)
            .Take(Math.Clamp(limit, 1, 500))
            .ToListAsync(ct);

    public async Task<IReadOnlyList<RoomInvitation>> GetGenerationDriftPendingAsync(
        int limit,
        CancellationToken ct = default)
        => await _context.RoomInvitations
            .AsNoTracking()
            .Where(invitation =>
                invitation.Status == RoomInvitationStatus.Pending
                && _context.Rooms.Any(room =>
                    room.Id == invitation.RoomId
                    && room.RosterGeneration != invitation.BaseRosterGeneration))
            .OrderBy(invitation => invitation.CreatedAt)
            .Take(Math.Clamp(limit, 1, 500))
            .ToListAsync(ct);

    public async Task AddAsync(RoomInvitation invitation, CancellationToken ct = default)
    {
        await _context.RoomInvitations.AddAsync(invitation, ct);
    }
}
