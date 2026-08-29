using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRoomRepository
{
    Task<Room?> GetByIdAsync(RoomId id, CancellationToken ct = default);
    Task AddAsync(Room room, CancellationToken ct = default);
    Task<IReadOnlyList<Room>> GetByMemberPageAsync(
        UserId userId,
        FeedCursor? cursor,
        int limit,
        CancellationToken ct = default);
    Task<IReadOnlyList<Room>> GetByIdsAsync(
        IReadOnlyCollection<RoomId> ids,
        CancellationToken ct = default);
}
