using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class RoomChatService(
    IRoomRepository rooms,
    IRoomMemberRepository roomMembers,
    IRoomChatRepository chat,
    ISessionRepository sessions,
    IUnitOfWork unitOfWork)
{
    private readonly IRoomRepository _rooms = rooms;
    private readonly IRoomMemberRepository _roomMembers = roomMembers;
    private readonly IRoomChatRepository _chat = chat;
    private readonly ISessionRepository _sessions = sessions;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<RoomChatMessage> PostAsync(
        RoomId roomId,
        Guid messageId,
        UserId authorUserId,
        Guid? authorSessionId,
        RoomChatAuthorKind authorKind,
        string body,
        IReadOnlyList<Guid>? recipientSessionIds = null,
        IReadOnlyList<Guid>? recipientUserIds = null,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        var message = await PostCoreAsync(
            roomId,
            messageId,
            authorUserId,
            authorSessionId,
            authorKind,
            body,
            recipientSessionIds,
            recipientUserIds,
            ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return message;
    }

    private async Task<RoomChatMessage> PostCoreAsync(
        RoomId roomId,
        Guid messageId,
        UserId authorUserId,
        Guid? authorSessionId,
        RoomChatAuthorKind authorKind,
        string body,
        IReadOnlyList<Guid>? recipientSessionIds,
        IReadOnlyList<Guid>? recipientUserIds,
        CancellationToken ct)
    {
        ValidateRecipientCount(recipientSessionIds?.Count ?? 0, recipientUserIds?.Count ?? 0);
        var normalizedSessionIds = NormalizeRecipients(recipientSessionIds, "session");
        var normalizedUserIds = NormalizeRecipients(recipientUserIds, "user");
        ValidateAttributionShape(authorSessionId, authorKind);
        if (authorSessionId is { } authorSession
            && normalizedSessionIds.Contains(authorSession))
        {
            throw new PolicyViolationException("Chat messages cannot target the author session.");
        }

        await _chat.AcquireMessageLockAsync(messageId, ct);
        await _chat.AcquireRoomLockAsync(roomId, ct);
        var room = await _rooms.GetByIdAsync(roomId, ct)
            ?? throw new NotFoundException(nameof(Room), roomId);
        if (!await _roomMembers.IsMemberAsync(roomId, authorUserId, ct)
            && !room.IsOwner(authorUserId))
        {
            throw new PolicyViolationException("Only room members may post chat.");
        }

        if (await ResolveReplayAsync(
                messageId,
                roomId,
                authorUserId,
                authorSessionId,
                authorKind,
                body,
                normalizedSessionIds,
                normalizedUserIds,
                ct) is { } replay)
        {
            return replay;
        }

        var lockedSessions = new Dictionary<Guid, Session>();
        var sessionIdsToLock = normalizedSessionIds
            .Concat(authorSessionId is { } id ? [id] : [])
            .Distinct()
            .Order()
            .ToArray();
        foreach (var lockedSessionId in sessionIdsToLock)
        {
            if (await _sessions.GetByIdForUpdateAsync(SessionId.From(lockedSessionId), ct)
                is { } session)
            {
                lockedSessions.Add(lockedSessionId, session);
            }
        }

        if (authorSessionId is { } sessionId)
        {
            if (!lockedSessions.TryGetValue(sessionId, out var session)
                || !session.IsOwner(authorUserId)
                || session.Status is not (SessionStatus.Live or SessionStatus.Reconnecting)
                || session.Scope != SessionScope.Room
                || session.RoomId != roomId)
            {
                throw new PolicyViolationException(
                    "Chat session attribution must name the author's session in this room.");
            }
        }

        var activeParticipantIds = new HashSet<UserId> { authorUserId };
        foreach (var recipientSessionId in normalizedSessionIds)
        {
            if (!lockedSessions.TryGetValue(recipientSessionId, out var recipientSession)
                || recipientSession.Status is not (SessionStatus.Live or SessionStatus.Reconnecting)
                || recipientSession.Scope != SessionScope.Room
                || recipientSession.RoomId != roomId)
            {
                throw new PolicyViolationException(
                    "Chat recipient sessions must be live or reconnecting sessions in this room.");
            }
            if (!room.IsOwner(recipientSession.OwnerUserId)
                && !activeParticipantIds.Contains(recipientSession.OwnerUserId)
                && !await _roomMembers.IsMemberAsync(roomId, recipientSession.OwnerUserId, ct))
            {
                throw new PolicyViolationException(
                    "Chat recipient sessions must be owned by active room participants.");
            }
            activeParticipantIds.Add(recipientSession.OwnerUserId);
        }

        foreach (var recipientUserId in normalizedUserIds)
        {
            var userId = UserId.From(recipientUserId);
            if (!room.IsOwner(userId)
                && !activeParticipantIds.Contains(userId)
                && !await _roomMembers.IsMemberAsync(roomId, userId, ct))
            {
                throw new PolicyViolationException(
                    "Chat recipient users must be the room owner or active room members.");
            }
            activeParticipantIds.Add(userId);
        }

        return await _chat.AddWithNextSeqAsync(
            roomId,
            messageId,
            authorUserId,
            authorSessionId,
            authorKind,
            normalizedSessionIds,
            normalizedUserIds,
            body,
            ct);
    }

    public async Task<RoomChatMessage?> GetByIdAsync(
        RoomId roomId,
        Guid messageId,
        UserId requesterUserId,
        CancellationToken ct = default)
    {
        var room = await _rooms.GetByIdAsync(roomId, ct)
            ?? throw new NotFoundException(nameof(Room), roomId);
        if (!await _roomMembers.IsMemberAsync(roomId, requesterUserId, ct)
            && !room.IsOwner(requesterUserId))
        {
            throw new PolicyViolationException("Only room members may read chat.");
        }
        var message = await _chat.GetByIdAsync(messageId, ct);
        return message?.RoomId == roomId ? message : null;
    }

    public async Task<RoomChatReadPage> ListTailAsync(
        RoomId roomId,
        UserId requesterUserId,
        long? beforeSeq,
        int limit,
        CancellationToken ct = default)
    {
        await EnsureCanReadAsync(roomId, requesterUserId, ct);
        var normalizedLimit = RoomChatReadPolicy.Normalize(limit);
        var candidates = await _chat.GetTailCandidatesAsync(
            roomId,
            beforeSeq,
            normalizedLimit,
            ct);
        return RoomChatReadPolicy.CreateTailPage(candidates, normalizedLimit);
    }

    private async Task EnsureCanReadAsync(
        RoomId roomId,
        UserId requesterUserId,
        CancellationToken ct)
    {
        var room = await _rooms.GetByIdAsync(roomId, ct)
            ?? throw new NotFoundException(nameof(Room), roomId);
        if (!await _roomMembers.IsMemberAsync(roomId, requesterUserId, ct)
            && !room.IsOwner(requesterUserId))
        {
            throw new PolicyViolationException("Only room members may read chat.");
        }
    }

    public async Task<RoomChatReadPage> ListAsync(
        RoomId roomId,
        UserId requesterUserId,
        long sinceSeq,
        int limit,
        CancellationToken ct = default)
    {
        await EnsureCanReadAsync(roomId, requesterUserId, ct);

        var normalizedLimit = RoomChatReadPolicy.Normalize(limit);
        var candidates = await _chat.GetPageCandidatesSinceAsync(
            roomId,
            sinceSeq,
            normalizedLimit,
            ct);
        return RoomChatReadPolicy.CreatePage(candidates, normalizedLimit);
    }

    private static Guid[] NormalizeRecipients(
        IReadOnlyList<Guid>? recipientIds,
        string recipientKind)
    {
        if (recipientIds is null || recipientIds.Count == 0)
        {
            return [];
        }
        if (recipientIds.Contains(Guid.Empty))
        {
            throw new PolicyViolationException(
                $"Chat recipient {recipientKind} IDs cannot be empty.");
        }
        if (recipientIds.Distinct().Count() != recipientIds.Count)
        {
            throw new PolicyViolationException(
                $"Chat recipient {recipientKind} IDs cannot contain duplicates.");
        }

        return recipientIds.Order().ToArray();
    }

    private async Task<RoomChatMessage?> ResolveReplayAsync(
        Guid messageId,
        RoomId roomId,
        UserId authorUserId,
        Guid? authorSessionId,
        RoomChatAuthorKind authorKind,
        string body,
        IReadOnlyList<Guid> recipientSessionIds,
        IReadOnlyList<Guid> recipientUserIds,
        CancellationToken ct)
    {
        var existing = await _chat.GetByIdAsync(messageId, ct);
        if (existing is null)
        {
            return null;
        }
        if (existing.RoomId != roomId
            || existing.AuthorUserId != authorUserId
            || existing.AuthorSessionId != authorSessionId
            || existing.AuthorKind != authorKind
            || existing.Body != body
            || !existing.RecipientSessionIds.SequenceEqual(recipientSessionIds)
            || !existing.RecipientUserIds.SequenceEqual(recipientUserIds))
        {
            throw new ConflictException(
                "Chat message ID is already in use with a different request fingerprint.");
        }

        return existing;
    }

    private static void ValidateAttributionShape(
        Guid? authorSessionId,
        RoomChatAuthorKind authorKind)
    {
        if (authorKind == RoomChatAuthorKind.Agent && authorSessionId is null)
        {
            throw new PolicyViolationException(
                "Agent chat attribution requires the originating room session.");
        }
        if (authorKind == RoomChatAuthorKind.Human && authorSessionId is not null)
        {
            throw new PolicyViolationException(
                "Human chat attribution cannot name an agent session.");
        }
    }

    private static void ValidateRecipientCount(int sessionCount, int userCount)
    {
        if (sessionCount > RoomInputRules.ChatRecipientMaxCount
            || userCount > RoomInputRules.ChatRecipientMaxCount - sessionCount)
        {
            throw new PolicyViolationException(
                $"Chat messages may target at most {RoomInputRules.ChatRecipientMaxCount} recipients.");
        }
    }
}
