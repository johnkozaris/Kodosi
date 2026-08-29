using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class RoomChatRepository(KodosiDbContext context) : IRoomChatRepository
{
    private static ReadOnlySpan<byte> MessageLockDomain => "kodosi-room-chat-message"u8;
    private readonly KodosiDbContext _context = context;

    public async Task AcquireMessageLockAsync(
        Guid messageId,
        CancellationToken ct = default)
    {
        EnsureActiveTransaction();

        var (key1, key2) = PostgresAdvisoryKey.Derive(MessageLockDomain, messageId);
        _ = await _context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT pg_advisory_xact_lock({key1}, {key2})",
            ct);
    }

    public async Task AcquireRoomLockAsync(
        RoomId roomId,
        CancellationToken ct = default)
    {
        EnsureActiveTransaction();
        _ = await _context.Database.ExecuteSqlInterpolatedAsync(
            $"SELECT 1 FROM rooms WHERE id = {roomId.Value} FOR UPDATE",
            ct);
    }

    public async Task<RoomChatMessage?> GetByIdAsync(
        Guid messageId,
        CancellationToken ct = default)
        => await _context.RoomChatMessages
            .FirstOrDefaultAsync(message => message.Id == messageId, ct);

    public async Task<RoomChatMessage> AddWithNextSeqAsync(
        RoomId roomId,
        Guid messageId,
        UserId authorUserId,
        Guid? authorSessionId,
        RoomChatAuthorKind authorKind,
        IReadOnlyList<Guid> recipientSessionIds,
        IReadOnlyList<Guid> recipientUserIds,
        string body,
        CancellationToken ct = default)
    {
        EnsureActiveTransaction();
        var normalizedRecipientSessionIds = recipientSessionIds.Order().ToArray();
        var normalizedRecipientUserIds = recipientUserIds.Order().ToArray();

        var maxSeq = await _context.RoomChatMessages
            .Where(m => m.RoomId == roomId)
            .MaxAsync(m => (long?)m.Seq, ct);
        var nextSeq = (maxSeq ?? 0) + 1;

        var message = RoomChatMessage.Create(
            messageId,
            roomId,
            authorUserId,
            authorSessionId,
            authorKind,
            normalizedRecipientSessionIds,
            normalizedRecipientUserIds,
            body,
            nextSeq);
        await _context.RoomChatMessages.AddAsync(message, ct);
        return message;
    }

    private void EnsureActiveTransaction()
    {
        if (_context.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "Room chat locking requires an active database transaction.");
        }
    }

    public async Task<IReadOnlyList<RoomChatMessage>> GetPageCandidatesSinceAsync(
        RoomId roomId,
        long sinceSeq,
        int limit,
        CancellationToken ct = default)
    {
        var candidateLimit = RoomChatReadPolicy.CandidateLimit(limit);
        return await _context.RoomChatMessages
            .FromSqlInterpolated($"""
                WITH RECURSIVE page AS (
                    (
                        SELECT
                            message.*,
                            1 AS page_position,
                            char_length(message.body)::bigint AS body_characters
                        FROM room_chat_messages AS message
                        WHERE message.room_id = {roomId.Value}
                          AND message.seq > {sinceSeq}
                        ORDER BY message.seq
                        LIMIT 1
                    )

                    UNION ALL

                    SELECT
                        message.*,
                        page.page_position + 1,
                        page.body_characters + char_length(message.body)
                    FROM page
                    JOIN LATERAL (
                        SELECT next_message.*
                        FROM room_chat_messages AS next_message
                        WHERE next_message.room_id = {roomId.Value}
                          AND next_message.seq > page.seq
                        ORDER BY next_message.seq
                        LIMIT 1
                    ) AS message ON TRUE
                    WHERE page.page_position < {candidateLimit}
                      AND page.body_characters <= {RoomChatReadPolicy.MaxPageBodyCharacters}
                )
                SELECT
                    page.id,
                    page.room_id,
                    page.author_user_id,
                    page.author_session_id,
                    page.author_kind,
                    page.recipient_session_ids,
                    page.recipient_user_ids,
                    page.body,
                    page.seq,
                    page.posted_at
                FROM page
                ORDER BY page.seq
                """)
            .AsNoTracking()
            .ToListAsync(ct);
    }

    public async Task<IReadOnlyList<RoomChatMessage>> GetTailCandidatesAsync(
        RoomId roomId,
        long? beforeSeq,
        int limit,
        CancellationToken ct = default)
    {
        var candidateLimit = RoomChatReadPolicy.CandidateLimit(limit);
        var upperBound = beforeSeq ?? long.MaxValue;
        var descending = await _context.RoomChatMessages
            .FromSqlInterpolated($"""
                WITH RECURSIVE page AS (
                    (
                        SELECT
                            message.*,
                            1 AS page_position,
                            char_length(message.body)::bigint AS body_characters
                        FROM room_chat_messages AS message
                        WHERE message.room_id = {roomId.Value}
                          AND message.seq < {upperBound}
                        ORDER BY message.seq DESC
                        LIMIT 1
                    )

                    UNION ALL

                    SELECT
                        message.*,
                        page.page_position + 1,
                        page.body_characters + char_length(message.body)
                    FROM page
                    JOIN LATERAL (
                        SELECT previous_message.*
                        FROM room_chat_messages AS previous_message
                        WHERE previous_message.room_id = {roomId.Value}
                          AND previous_message.seq < page.seq
                        ORDER BY previous_message.seq DESC
                        LIMIT 1
                    ) AS message ON TRUE
                    WHERE page.page_position < {candidateLimit}
                      AND page.body_characters <= {RoomChatReadPolicy.MaxPageBodyCharacters}
                )
                SELECT
                    page.id,
                    page.room_id,
                    page.author_user_id,
                    page.author_session_id,
                    page.author_kind,
                    page.recipient_session_ids,
                    page.recipient_user_ids,
                    page.body,
                    page.seq,
                    page.posted_at
                FROM page
                ORDER BY page.seq DESC
                """)
            .AsNoTracking()
            .ToListAsync(ct);
        return descending;
    }
}
