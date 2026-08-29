using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRoomTaskRepository
{
    Task<RoomTask?> GetByIdAsync(Guid id, CancellationToken ct = default);
    Task<RoomTask?> GetByIdForUpdateAsync(Guid id, CancellationToken ct = default);

    Task<IReadOnlyList<RoomTask>> GetByRoomPageAsync(
        RoomId roomId,
        RoomTaskStatus? statusFilter,
        Guid? assigneeSessionFilter,
        int offset,
        int candidateLimit,
        CancellationToken ct = default);

    Task<RoomTask> AddIdempotentAsync(
        RoomTask task,
        CancellationToken ct = default);
}
