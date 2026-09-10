using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISharedRoomAuthorizationRepository
{
    Task<IReadOnlyList<RoomId>> GetSharedActiveRoomIdsAsync(
        UserId firstUserId,
        UserId secondUserId,
        CancellationToken ct = default);

    Task<IReadOnlyList<RoomId>> GetHistoricalArtifactRoomIdsAsync(
        UserId readerUserId,
        UserId authorUserId,
        CancellationToken ct = default);
}
