using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRoomMemberRepository
{
    Task<bool> IsMemberAsync(RoomId roomId, UserId userId, CancellationToken ct = default);
    Task<IReadOnlyList<RoomId>> GetActiveRoomIdsForUserAsync(
        UserId userId,
        IReadOnlyList<RoomId> roomIds,
        CancellationToken ct = default);
    Task<RoomMember?> GetAsync(RoomId roomId, UserId userId, CancellationToken ct = default);
    Task AddAsync(RoomMember member, CancellationToken ct = default);
    Task<IReadOnlyList<UserId>> GetMemberUserIdsAsync(RoomId roomId, CancellationToken ct = default);
    Task<IReadOnlyList<RoomMember>> GetActiveByRoomAsync(
        RoomId roomId,
        CancellationToken ct = default);

    Task<IReadOnlyList<RoomMember>> GetAdmissionProofPageAfterAsync(
        RoomId roomId,
        Guid? afterUserId,
        int limit,
        CancellationToken ct = default);

    Task<IReadOnlyList<UserId>> GetRoomPeerUserIdsAsync(
        UserId userId,
        CancellationToken ct = default);
}
