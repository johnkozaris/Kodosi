using Kodosi.Data;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Missions;

/// <summary>One person in one room: the owner or a member, and an invitee when asked.</summary>
internal sealed class RoomPerson
{
    public Guid MissionId { get; init; }
    public Guid UserId { get; init; }
}

/// <summary>
/// The people of each room. Each read of room membership uses this one query, and PostgreSQL applies its filter in
/// each branch, so that each branch uses its index.
/// </summary>
internal static class RoomMembership
{
    public static IQueryable<RoomPerson> People(KodosiDbContext db, bool invited = false)
    {
        var people = db.Missions.Select(x => new RoomPerson { MissionId = x.Id, UserId = x.OwnerUserId })
            .Concat(db.MissionMembers.Select(x => new RoomPerson { MissionId = x.MissionId, UserId = x.UserId }));
        return invited ? people.Concat(db.MissionInvitations.Select(x => new RoomPerson { MissionId = x.MissionId, UserId = x.UserId })) : people;
    }

    public static IQueryable<Guid> RoomsOf(KodosiDbContext db, Guid userId, bool invited = false) =>
        People(db, invited).Where(x => x.UserId == userId).Select(x => x.MissionId);

    public static IQueryable<Guid> PeopleOf(KodosiDbContext db, Guid roomId, bool invited = false) =>
        People(db, invited).Where(x => x.MissionId == roomId).Select(x => x.UserId);

    /// <summary>Tells the people of each room of the account that the room changed, for example after a change of its devices.</summary>
    public static async Task NotifyRoomsOfAsync(KodosiDbContext db, ConnectionDirectory connections, Guid userId, CancellationToken ct)
    {
        var rooms = RoomsOf(db, userId);
        var people = await People(db).Where(x => rooms.Contains(x.MissionId)).Select(x => new { x.MissionId, x.UserId }).Distinct().ToListAsync(ct);
        foreach (var room in people.GroupBy(x => x.MissionId)) connections.Notify(room.Select(x => x.UserId), "missions", room.Key);
    }
}
