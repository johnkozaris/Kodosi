using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class RoomSessionFeedService(
    ISessionRepository sessions,
    IRoomMemberRepository roomMembers,
    IAccessOverrideRepository overrides,
    ISessionViewerDismissalRepository dismissals,
    ILiveSessionStateDirectory runtimes)
{
    private const int MaxLimit = 100;

    private readonly ISessionRepository _sessions = sessions;
    private readonly IRoomMemberRepository _roomMembers = roomMembers;
    private readonly IAccessOverrideRepository _overrides = overrides;
    private readonly ISessionViewerDismissalRepository _dismissals = dismissals;
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;

    public async Task<PaginatedFeedResponse<SessionCardResponse>> GetRoomFeedAsync(
    RoomId roomId,
    UserId requestorId,
    string? cursor = null,
    int limit = 20,
    ToolKind? toolKindFilter = null,
    string? sortBy = null,
    DateTimeOffset? since = null,
    CancellationToken ct = default)
    {
        limit = Math.Clamp(limit, 1, MaxLimit);
        var sortByParticipants = IsParticipantSort(sortBy);
        if (!await _roomMembers.IsMemberAsync(roomId, requestorId, ct))
        {
            throw new PolicyViolationException("Only room members may access this room feed.");
        }

        if (sortByParticipants)
        {
            if (!string.IsNullOrWhiteSpace(cursor))
            {
                throw new DomainException(
                    "Participant-sorted room feeds do not support cursor pagination.");
            }

            return await GetParticipantSortedFeedAsync(
                roomId,
                requestorId,
                limit,
                toolKindFilter,
                since,
                ct);
        }

        var decodedCursor = FeedCursor.Decode(cursor);
        var projections = await _sessions.GetLiveByRoomProjectedAsync(
            roomId, decodedCursor, limit, toolKindFilter, since, ct);
        var accessBySessionId = await ResolveFeedAccessAsync(projections, requestorId, ct);

        var overflowed = projections.Count > limit;
        var hasMore = overflowed;
        var items = overflowed ? projections.Take(limit).ToList() : projections;

        var cards = items
            .Where(projection => accessBySessionId.ContainsKey(projection.Id))
            .Select(projection => projection.ToCardResponse(accessBySessionId[projection.Id], _runtimes))
            .ToList();

        string? nextCursor = null;
        if (hasMore && items.Count > 0)
        {
            var last = items[^1];
            nextCursor = new FeedCursor(last.StartedAt, last.Id).Encode();
        }

        return new PaginatedFeedResponse<SessionCardResponse>(cards, nextCursor, hasMore);
    }

    private async Task<PaginatedFeedResponse<SessionCardResponse>> GetParticipantSortedFeedAsync(
        RoomId roomId,
        UserId requestorId,
        int limit,
        ToolKind? toolKindFilter,
        DateTimeOffset? since,
        CancellationToken ct)
    {
        var (projections, scanCapped) = await LoadParticipantSortProjectionsAsync(
            roomId,
            toolKindFilter,
            since,
            ct);
        var accessBySessionId = await ResolveFeedAccessAsync(projections, requestorId, ct);




        var ordered = projections
            .Where(projection => accessBySessionId.ContainsKey(projection.Id))
            .Select(projection => new ParticipantFeedEntry(
                projection,
                projection.ToCardResponse(accessBySessionId[projection.Id], _runtimes)))
            .OrderByDescending(entry => entry.Card.ParticipantCount)
            .ThenByDescending(entry => entry.Projection.StartedAt)
            .ThenByDescending(entry => entry.Projection.Id)
            .ToList();

        var cards = ordered.Count > limit
            ? ordered.Take(limit).Select(entry => entry.Card).ToList()
            : ordered.Select(entry => entry.Card).ToList();

        return new PaginatedFeedResponse<SessionCardResponse>(
            cards,
            NextCursor: null,
            HasMore: false,
            Truncated: scanCapped || ordered.Count > limit);
    }




    private async Task<(IReadOnlyList<SessionCardProjection> Projections, bool ScanCapped)>
        LoadParticipantSortProjectionsAsync(
            RoomId roomId,
            ToolKind? toolKindFilter,
            DateTimeOffset? since,
            CancellationToken ct)
    {
        var projections = await _sessions.GetLiveByRoomProjectedAsync(
            roomId,
            cursor: null,
            MaxLimit,
            toolKindFilter,
            since,
            ct);
        return projections.Count > MaxLimit
            ? (projections.Take(MaxLimit).ToList(), true)
            : (projections, false);
    }



    private async Task<IReadOnlyDictionary<Guid, AccessLevel>> ResolveFeedAccessAsync(
        IReadOnlyList<SessionCardProjection> projections,
        UserId actorUserId,
        CancellationToken ct)
    {
        if (projections.Count == 0)
        {
            return new Dictionary<Guid, AccessLevel>();
        }

        var sessionIds = projections
            .Select(projection => SessionId.From(projection.Id))
            .Distinct()
            .ToList();
        var activeOverrides = await _overrides.GetActiveForActorAsync(actorUserId, sessionIds, ct);
        var dismissedSessionIds = await _dismissals.GetDismissedSessionIdsAsync(
            actorUserId,
            sessionIds,
            ct);
        var overrideBySessionId = new Dictionary<SessionId, AccessLevel>(activeOverrides.Count);
        foreach (var activeOverride in activeOverrides)
        {
            overrideBySessionId[activeOverride.SessionId] = activeOverride.AccessLevel;
        }

        var memberRoomIds = await LoadMemberRoomIdsAsync(projections, actorUserId, ct);

        var resolvedAccess = new Dictionary<Guid, AccessLevel>(projections.Count);
        foreach (var projection in projections)
        {
            var sessionId = SessionId.From(projection.Id);
            if (projection.OwnerUserId != actorUserId.Value
                && dismissedSessionIds.Contains(sessionId))
            {
                continue;
            }

            AccessLevel? overrideLevel = overrideBySessionId.TryGetValue(sessionId, out var resolvedOverride)
                ? resolvedOverride
                : null;
            if (AccessResolver.TryResolveAccess(
                projection.OwnerUserId == actorUserId.Value,
                projection.Scope,
                projection.DefaultAccess,
                isFriend: false,
                projection.RoomId is Guid roomId
                    && memberRoomIds.Contains(RoomId.From(roomId)),
                overrideLevel,
                out var accessLevel))
            {
                resolvedAccess[projection.Id] = accessLevel;
            }
        }

        return resolvedAccess;
    }

    private async Task<HashSet<RoomId>> LoadMemberRoomIdsAsync(
        IReadOnlyList<SessionCardProjection> projections,
        UserId actorUserId,
        CancellationToken ct)
    {
        var roomIds = projections
            .Where(projection => projection.Scope == SessionScope.Room && projection.RoomId.HasValue)
            .Select(projection => RoomId.From(projection.RoomId!.Value))
            .Distinct()
            .ToList();
        if (roomIds.Count == 0)
        {
            return [];
        }

        return
        [
            .. await _roomMembers.GetActiveRoomIdsForUserAsync(
                actorUserId,
                roomIds,
                ct)
        ];
    }

    private static bool IsParticipantSort(string? sortBy)
        => string.Equals(sortBy, "participants", StringComparison.OrdinalIgnoreCase);

    private sealed record ParticipantFeedEntry(
        SessionCardProjection Projection,
        SessionCardResponse Card);
}
