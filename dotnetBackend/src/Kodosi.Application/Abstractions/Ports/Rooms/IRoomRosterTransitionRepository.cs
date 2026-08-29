using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRoomRosterTransitionRepository
{
    Task AddAsync(RoomRosterTransition transition, CancellationToken ct = default);

    Task<IReadOnlyList<RoomRosterTransition>> GetPageAfterAsync(
        RoomId roomId,
        long afterGeneration,
        int limit,
        CancellationToken ct = default);
}
