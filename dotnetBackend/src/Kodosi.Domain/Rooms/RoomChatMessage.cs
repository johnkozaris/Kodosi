namespace Kodosi.Domain;

public enum RoomChatAuthorKind
{
    Human,
    Agent,
}

public sealed class RoomChatMessage
{
    public Guid Id { get; private set; }
    public RoomId RoomId { get; private set; } = default!;
    public UserId AuthorUserId { get; private set; } = default!;
    public Guid? AuthorSessionId { get; private set; }
    public RoomChatAuthorKind AuthorKind { get; private set; }
    public Guid[] RecipientSessionIds { get; private set; } = [];
    public Guid[] RecipientUserIds { get; private set; } = [];
    public string Body { get; private set; } = string.Empty;
    public long Seq { get; private set; }
    public DateTimeOffset PostedAt { get; private set; }

    private RoomChatMessage() { }

    public static RoomChatMessage Create(
        Guid id,
        RoomId roomId,
        UserId authorUserId,
        Guid? authorSessionId,
        RoomChatAuthorKind authorKind,
        IReadOnlyCollection<Guid>? recipientSessionIds,
        IReadOnlyCollection<Guid>? recipientUserIds,
        string body,
        long seq)
    {
        if (id == Guid.Empty)
        {
            throw new DomainException("Chat message ID is required.");
        }
        if (string.IsNullOrWhiteSpace(body))
        {
            throw new DomainException("Chat message body cannot be empty.");
        }

        if (body.Length > RoomInputRules.EncryptedContentMaxLength)
        {
            throw new DomainException(
                $"Encrypted chat payload exceeds {RoomInputRules.EncryptedContentMaxLength} chars.");
        }

        ValidateRecipientCount(recipientSessionIds?.Count ?? 0, recipientUserIds?.Count ?? 0);
        var normalizedSessionIds = NormalizeRecipients(recipientSessionIds, "session");
        var normalizedUserIds = NormalizeRecipients(recipientUserIds, "user");
        if (authorSessionId is { } sessionId && normalizedSessionIds.Contains(sessionId))
        {
            throw new DomainException("Chat messages cannot target the author session.");
        }

        return new RoomChatMessage
        {
            Id = id,
            RoomId = roomId,
            AuthorUserId = authorUserId,
            AuthorSessionId = authorSessionId,
            AuthorKind = authorKind,
            RecipientSessionIds = normalizedSessionIds,
            RecipientUserIds = normalizedUserIds,
            Body = body,
            Seq = seq,
            PostedAt = DateTimeOffset.UtcNow,
        };
    }

    private static Guid[] NormalizeRecipients(
        IReadOnlyCollection<Guid>? recipientIds,
        string recipientKind)
    {
        if (recipientIds is null || recipientIds.Count == 0)
        {
            return [];
        }
        if (recipientIds.Contains(Guid.Empty))
        {
            throw new DomainException($"Chat recipient {recipientKind} IDs cannot be empty.");
        }
        if (recipientIds.Distinct().Count() != recipientIds.Count)
        {
            throw new DomainException($"Chat recipient {recipientKind} IDs cannot contain duplicates.");
        }

        return recipientIds.Order().ToArray();
    }

    private static void ValidateRecipientCount(int sessionCount, int userCount)
    {
        if (sessionCount > RoomInputRules.ChatRecipientMaxCount
            || userCount > RoomInputRules.ChatRecipientMaxCount - sessionCount)
        {
            throw new DomainException(
                $"Chat messages may target at most {RoomInputRules.ChatRecipientMaxCount} recipients.");
        }
    }
}
