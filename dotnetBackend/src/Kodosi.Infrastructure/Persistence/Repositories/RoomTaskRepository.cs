using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class RoomTaskRepository(KodosiDbContext context) : IRoomTaskRepository
{
    private readonly KodosiDbContext _context = context;

    public Task<RoomTask?> GetByIdAsync(Guid id, CancellationToken ct = default)
        => _context.RoomTasks.FirstOrDefaultAsync(t => t.Id == id, ct);

    public async Task<RoomTask?> GetByIdForUpdateAsync(
        Guid id,
        CancellationToken ct = default)
    {
        if (_context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "Room task row lock requires an active database transaction.");
        }
        _ = await _context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT 1 FROM room_tasks WHERE id = {id} FOR UPDATE",
            ct);
        return await GetByIdAsync(id, ct);
    }

    public async Task<IReadOnlyList<RoomTask>> GetByRoomPageAsync(
        RoomId roomId,
        RoomTaskStatus? statusFilter,
        Guid? assigneeSessionFilter,
        int offset,
        int candidateLimit,
        CancellationToken ct = default)
    {
        var query = _context.RoomTasks.AsNoTracking().Where(t => t.RoomId == roomId);
        if (statusFilter is { } status)
        {
            query = query.Where(t => t.Status == status);
        }
        if (assigneeSessionFilter is { } assignee)
        {
            query = query.Where(t => t.AssignedSessionId == assignee);
        }

        return await query
            .OrderByDescending(t => t.UpdatedAt)
            .ThenByDescending(t => t.Id)
            .Skip(offset)
            .Take(candidateLimit)
            .ToListAsync(ct);
    }

    public async Task<RoomTask> AddIdempotentAsync(
        RoomTask task,
        CancellationToken ct = default)
    {
        _ = await _context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT 1 FROM rooms WHERE id = {task.RoomId.Value} FOR UPDATE",
            ct);
        var existing = await _context.RoomTasks
            .FirstOrDefaultAsync(candidate => candidate.Id == task.Id, ct);
        if (existing is not null)
        {
            if (existing.RoomId != task.RoomId
                || existing.CreatedByUserId != task.CreatedByUserId
                || existing.Title != task.Title
                || existing.Description != task.Description
                || existing.AssignedSessionId != task.AssignedSessionId
                || existing.AssignedSessionIncarnationId != task.AssignedSessionIncarnationId
                || existing.DueAt != task.DueAt)
            {
                throw new ConflictException(
                    "Room task ID is already in use with a different request fingerprint.");
            }
            return existing;
        }

        await _context.RoomTasks.AddAsync(task, ct);
        return task;
    }
}
