using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRoomMutationReceiptRepository
{
    Task AcquireAsync(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        CancellationToken ct = default);

    Task<RoomMutationReceipt?> GetAsync(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        CancellationToken ct = default);

    Task<RoomMutationReceipt?> FindAsync(
        UserId actorUserId,
        RoomMutationOperation operation,
        Guid requestId,
        CancellationToken ct = default);

    Task AddAsync(
        RoomMutationReceipt receipt,
        CancellationToken ct = default);

    Task AddSessionEffectsAsync(
        IReadOnlyCollection<RoomMutationSessionEffect> effects,
        CancellationToken ct = default);

    Task<IReadOnlyList<RoomMutationSessionEffect>> GetSessionEffectsAsync(
        UserId actorUserId,
        Guid requestId,
        CancellationToken ct = default);
}
