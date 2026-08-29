using Kodosi.Domain;

namespace Kodosi.Application;

public interface IRoomChatRepository
{
    Task AcquireMessageLockAsync(Guid messageId, CancellationToken ct = default);

    Task AcquireRoomLockAsync(RoomId roomId, CancellationToken ct = default);

    Task<RoomChatMessage?> GetByIdAsync(
        Guid messageId,
        CancellationToken ct = default);

    Task<RoomChatMessage> AddWithNextSeqAsync(
        RoomId roomId,
        Guid messageId,
        UserId authorUserId,
        Guid? authorSessionId,
        RoomChatAuthorKind authorKind,
        IReadOnlyList<Guid> recipientSessionIds,
        IReadOnlyList<Guid> recipientUserIds,
        string body,
        CancellationToken ct = default);

    Task<IReadOnlyList<RoomChatMessage>> GetPageCandidatesSinceAsync(
        RoomId roomId,
        long sinceSeq,
        int limit,
        CancellationToken ct = default);

    Task<IReadOnlyList<RoomChatMessage>> GetTailCandidatesAsync(
        RoomId roomId,
        long? beforeSeq,
        int limit,
        CancellationToken ct = default);
}
