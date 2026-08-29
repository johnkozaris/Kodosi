using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionRoomResolver(IRoomMemberRepository roomMembers)
{
    private readonly IRoomMemberRepository _roomMembers = roomMembers;

    public async Task<RoomId?> ResolveAsync(
        UserId ownerId,
        SessionScope scope,
        Guid? roomId,
        CancellationToken ct = default)
    {
        if (scope != SessionScope.Room)
        {
            return null;
        }

        if (!roomId.HasValue)
        {
            throw new PolicyViolationException(
                "Room sessions require a selected room.");
        }

        var resolvedRoomId = RoomId.From(roomId.Value);
        if (!await _roomMembers.IsMemberAsync(resolvedRoomId, ownerId, ct))
        {
            throw new PolicyViolationException(
                "Room sessions require membership in the selected room.");
        }

        return resolvedRoomId;
    }
}
