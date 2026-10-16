using Kodosi.Data;
using Kodosi.Missions;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Sessions;

internal sealed class TerminalPerson
{
    public Guid SessionId { get; init; }
    public Guid UserId { get; init; }
}

internal static class TerminalAccess
{
    public static IQueryable<TerminalPerson> People(KodosiDbContext db) =>
        db.Sessions.Select(x => new TerminalPerson { SessionId = x.Id, UserId = x.OwnerUserId })
            .Concat(from session in db.Sessions
                    where session.MissionId == null
                    join member in db.SessionMembers on session.Id equals member.SessionId
                    select new TerminalPerson { SessionId = session.Id, UserId = member.UserId })
            .Concat(from session in db.Sessions
                    join person in RoomMembership.People(db) on session.MissionId equals (Guid?)person.MissionId
                    select new TerminalPerson { SessionId = session.Id, UserId = person.UserId });

    public static IQueryable<Session> UsableBy(this IQueryable<Session> sessions, KodosiDbContext db, Guid userId)
    {
        var people = People(db);
        return sessions.Where(session => people.Any(x => x.SessionId == session.Id && x.UserId == userId));
    }

    public static async Task<HashSet<Guid>> PeopleAsync(KodosiDbContext db, Guid sessionId, CancellationToken ct) =>
        (await People(db).Where(x => x.SessionId == sessionId).Select(x => x.UserId).ToListAsync(ct)).ToHashSet();

    public static async Task<ILookup<Guid, Guid>> PeopleAsync(KodosiDbContext db, IReadOnlyCollection<Guid> sessionIds, CancellationToken ct)
    {
        if (sessionIds.Count == 0) return Array.Empty<TerminalPerson>().ToLookup(x => x.SessionId, x => x.UserId);
        var people = await People(db).Where(x => sessionIds.Contains(x.SessionId)).Select(x => new { x.SessionId, x.UserId }).Distinct().ToListAsync(ct);
        return people.ToLookup(x => x.SessionId, x => x.UserId);
    }

    public static async Task RevokeAsync(KodosiDbContext db, ConnectionDirectory connections, IReadOnlyCollection<Session> sessions,
        CancellationToken ct, Func<Guid, string, bool>? keepDevice = null)
    {
        foreach (var session in sessions.Where(x => x.Ended)) connections.RemoveSession(session.Id);
        var live = sessions.Where(x => !x.Ended).Select(x => x.Id).ToArray();
        if (live.Length == 0) return;
        var people = await PeopleAsync(db, live, ct);
        foreach (var id in live)
        {
            var allowed = people[id].ToHashSet();
            connections.Revoke(id, (user, device) => allowed.Contains(user) && (keepDevice?.Invoke(user, device) ?? true));
        }
    }
}
