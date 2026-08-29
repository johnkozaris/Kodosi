using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRoomInvitationRepository
{
    Task<RoomInvitation?> GetByIdAsync(Guid id, CancellationToken ct = default);
    Task<IReadOnlyList<RoomInvitation>> GetByIdsAsync(
        IReadOnlyCollection<Guid> ids,
        CancellationToken ct = default);
    Task<RoomInvitationLockUsers?> GetLockUsersAsync(
        Guid id,
        CancellationToken ct = default);

    Task<RoomInvitation?> GetPendingAsync(
        RoomId roomId,
        UserId inviteeUserId,
        CancellationToken ct = default);

    Task<IReadOnlyList<RoomInvitation>> GetIncomingAsync(
        UserId userId,
        int limit,
        CancellationToken ct = default);

    Task<IReadOnlyList<RoomInvitation>> GetOutgoingAsync(
        UserId userId,
        int limit,
        CancellationToken ct = default);

    Task<IReadOnlyList<RoomInvitation>> GetPendingByRoomAsync(
        RoomId roomId,
        CancellationToken ct = default);

    Task<IReadOnlyList<RoomInvitation>> GetExpiredPendingAsync(
        DateTimeOffset now,
        int limit,
        CancellationToken ct = default);

    Task<IReadOnlyList<RoomInvitation>> GetGenerationDriftPendingAsync(
        int limit,
        CancellationToken ct = default);

    Task AddAsync(RoomInvitation invitation, CancellationToken ct = default);
}

public readonly record struct RoomInvitationLockUsers(
    RoomId RoomId,
    UserId OwnerUserId,
    UserId InviteeUserId);
