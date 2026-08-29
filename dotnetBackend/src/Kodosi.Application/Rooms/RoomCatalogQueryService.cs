using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record RoomCatalogPage(
    IReadOnlyList<Room> Items,
    string? NextCursor,
    bool HasMore);

public sealed record RoomAdmissionProofPage(
    IReadOnlyList<RoomMember> Items,
    Guid? NextUserId,
    bool HasMore);

public sealed record RoomRosterTransitionPage(
    IReadOnlyList<RoomRosterTransition> Items,
    long? NextGeneration,
    bool HasMore);

public sealed class RoomCatalogQueryService(
    IRoomRepository rooms,
    IRoomMemberRepository members,
    IRoomRosterTransitionRepository transitions)
{
    private const int MaxRoomLimit = 3;
    private const int MaxAdmissionProofLimit = 4;
    private const int MaxTransitionLimit = 8;

    private readonly IRoomRepository _rooms = rooms;
    private readonly IRoomMemberRepository _members = members;
    private readonly IRoomRosterTransitionRepository _transitions = transitions;

    public async Task<RoomCatalogPage> ListAsync(
        UserId userId,
        string? cursor,
        int? limit,
        CancellationToken ct = default)
    {
        var decodedCursor = FeedCursor.Decode(cursor);
        var normalizedLimit = Math.Clamp(limit ?? MaxRoomLimit, 1, MaxRoomLimit);
        var candidates = await _rooms.GetByMemberPageAsync(
            userId,
            decodedCursor,
            normalizedLimit + 1,
            ct);
        var pageRooms = candidates.Take(normalizedLimit).ToList();
        var hasMore = candidates.Count > pageRooms.Count;
        return new RoomCatalogPage(
            pageRooms,
            hasMore
                ? new FeedCursor(pageRooms[^1].CreatedAt, pageRooms[^1].Id.Value).Encode()
                : null,
            hasMore);
    }

    public async Task<RoomAdmissionProofPage?> ListAdmissionProofsAsync(
        RoomId roomId,
        UserId userId,
        Guid? afterUserId,
        int? limit,
        CancellationToken ct = default)
    {
        var room = await _rooms.GetByIdAsync(roomId, ct);
        if (room is null || !await _members.IsMemberAsync(roomId, userId, ct))
        {
            return null;
        }
        var normalizedLimit = Math.Clamp(
            limit ?? MaxAdmissionProofLimit,
            1,
            MaxAdmissionProofLimit);
        var candidates = await _members.GetAdmissionProofPageAfterAsync(
            roomId,
            afterUserId,
            normalizedLimit + 1,
            ct);
        var items = candidates.Take(normalizedLimit).ToList();
        var hasMore = candidates.Count > items.Count;
        return new RoomAdmissionProofPage(
            items,
            hasMore ? items[^1].UserId.Value : null,
            hasMore);
    }

    public async Task<RoomRosterTransitionPage?> ListTransitionsAsync(
        RoomId roomId,
        UserId userId,
        long afterGeneration,
        int? limit,
        CancellationToken ct = default)
    {
        if (afterGeneration < 0)
        {
            throw new DomainException("Roster transition generation cannot be negative.");
        }
        var room = await _rooms.GetByIdAsync(roomId, ct);
        if (room is null || !await _members.IsMemberAsync(roomId, userId, ct))
        {
            return null;
        }
        var normalizedLimit = Math.Clamp(
            limit ?? MaxTransitionLimit,
            1,
            MaxTransitionLimit);
        var candidates = await _transitions.GetPageAfterAsync(
            roomId,
            afterGeneration,
            normalizedLimit + 1,
            ct);
        var items = candidates.Take(normalizedLimit).ToList();
        var hasMore = candidates.Count > items.Count;
        return new RoomRosterTransitionPage(
            items,
            hasMore ? items[^1].Generation : null,
            hasMore);
    }
}
