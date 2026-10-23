using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Missions;
using Kodosi.Plans;
using Kodosi.Security;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Sessions;

public sealed class SessionService(KodosiDbContext db, ConnectionDirectory connections, MissionService missions, TimeProvider clock, PlanCatalog plans)
{
    // The record of an ended terminal stays so that a late request cannot reuse its identity.
    private static readonly TimeSpan EndedRecordLife = TimeSpan.FromMinutes(2);

    public async Task<Session> AuthorizedAsync(Guid id, Guid userId, CancellationToken ct) =>
        await db.Sessions.Where(x => x.Id == id && !x.Ended).UsableBy(db, userId).SingleOrDefaultAsync(ct) ?? throw ApiException.Missing();

    public async Task<Session> OwnerAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var session = await AuthorizedAsync(id, userId, ct);
        return session.OwnerUserId == userId ? session : throw ApiException.Forbidden("Only the owner can change this terminal.");
    }

    public async Task<Session> HostAsync(Guid id, Guid userId, string deviceId, CancellationToken ct)
    {
        var session = await OwnerAsync(id, userId, ct);
        return session.HostDeviceId == deviceId ? session : throw ApiException.Forbidden("Only the computer that hosts this terminal can change it.");
    }

    public async Task<SessionDto> DescribeAsync(Session session, Guid viewer, CancellationToken ct) =>
        (await DescribeAsync(db.Sessions.Where(x => x.Id == session.Id), viewer, 1, ct)).Single();

    // A terminal that someone asked to close leaves every list but that of the computer that closes it.
    public async Task<IReadOnlyList<SessionDto>> ListAsync(Guid userId, string deviceId, CancellationToken ct)
    {
        var usable = TerminalAccess.People(db).Where(x => x.UserId == userId).Select(x => x.SessionId);
        var listed = db.Sessions.Where(x => !x.Ended && usable.Contains(x.Id)
            && (x.CloseSignature == null || (x.OwnerUserId == userId && x.HostDeviceId == deviceId)));
        var sessions = await DescribeAsync(listed, userId, Limits.SessionCatalog + 1, ct);
        if (sessions.Count > Limits.SessionCatalog) throw ApiException.Conflict("There are too many terminals to list.");
        return sessions;
    }

    private async Task<List<SessionDto>> DescribeAsync(IQueryable<Session> sessions, Guid viewer, int most, CancellationToken ct)
    {
        var rows = await (from session in sessions.AsNoTracking()
                          join owner in db.Users on session.OwnerUserId equals owner.Id
                          join room in db.Missions on session.MissionId equals (Guid?)room.Id into rooms
                          from room in rooms.DefaultIfEmpty()
                          orderby session.CreatedAt, session.Id
                          select new
                          {
                              Session = session, OwnerName = owner.DisplayName, MissionName = room == null ? null : room.Name,
                              Hidden = db.HiddenSessions.Any(x => x.SessionId == session.Id && x.UserId == viewer),
                          })
            .Take(most).ToListAsync(ct);
        var people = await TerminalAccess.PeopleAsync(db, rows.Select(x => x.Session.Id).ToArray(), ct);
        return rows.Select(row =>
        {
            var session = row.Session;
            var (online, status) = connections.Live(session.Id);
            return new SessionDto(session.Id, session.IncarnationId, session.Name, session.OwnerUserId, row.OwnerName, session.HostDeviceId,
                session.HostName, session.MissionId, row.MissionName, people[session.Id].Where(user => user != session.OwnerUserId).ToArray(),
                session.AuthorizationRevision, online, status, session.RoomSequence,
                session.RoomSignature is { } signature ? new RoomChoice(session.RoomDeviceId!, Convert.ToBase64String(signature)) : null,
                row.Hidden && !online,
                session.CloseSignature is { } closing ? new CloseRequest(session.CloseUserId!.Value, session.CloseDeviceId!, Convert.ToBase64String(closing)) : null);
        }).ToList();
    }

    public async Task<SessionDto> CreateAsync(Guid userId, ApprovedDevice device, CreateSession body, CancellationToken ct)
    {
        Limits.Id(body.Id, "Terminal ID"); Limits.Id(body.IncarnationId, "Incarnation ID");
        if (body.HostDeviceId != device.Id) throw ApiException.Forbidden("Only the computer that hosts a terminal can add it.");
        var existing = await db.Sessions.SingleOrDefaultAsync(x => x.Id == body.Id, ct);
        if (existing is not null)
        {
            if (existing.Ended || existing.OwnerUserId != userId || existing.HostDeviceId != device.Id || existing.IncarnationId != body.IncarnationId)
                throw ApiException.Conflict("This terminal ID is already in use.");
            return await DescribeAsync(existing, userId, ct);
        }
        var live = db.Sessions.Where(x => x.OwnerUserId == userId && !x.Ended);
        if (await live.CountAsync(x => x.CloseSignature == null, ct) >= Limits.MaxSessionsPerUser)
            throw ApiException.Conflict("You have too many terminals.");
        if (await live.CountAsync(ct) >= Limits.MaxLiveSessionsPerUser)
            throw ApiException.Conflict("Too many of your terminals wait to close on offline computers. Remove the computers that do not come back.");
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        if (body.MissionId is { } missionId)
        {
            await missions.RequireMemberForUpdateAsync(missionId, userId, ct);
            await missions.RequireSharedRoomAsync(missionId, ct);
        }
        var now = clock.GetUtcNow();
        var session = new Session
        {
            Id = body.Id,
            IncarnationId = body.IncarnationId,
            OwnerUserId = userId,
            HostDeviceId = device.Id,
            HostName = Limits.Text(body.HostName, "Host name", 128),
            Name = Limits.Text(body.Name, "Session name", 128),
            MissionId = body.MissionId,
            ExpiresAt = now + EndedRecordLife,
            CreatedAt = now
        };
        db.Sessions.Add(session); await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        return await DescribeAndNotifyAsync(session, ct);
    }

    public async Task<SessionDto> RenameAsync(Guid id, Guid userId, RenameSession body, CancellationToken ct)
    {
        var session = await OwnerAsync(id, userId, ct); Match(session, body.IncarnationId, body.ExpectedRevision);
        session.Name = Limits.Text(body.Name, "Session name", 128);
        await db.SaveChangesAsync(ct);
        return await DescribeAndNotifyAsync(session, ct);
    }

    public async Task<SessionDto> AttachAsync(Guid id, Guid userId, ApprovedDevice device, AttachSession body, CancellationToken ct)
    {
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        if (body.MissionId is { } missionId)
        {
            await missions.RequireMemberForUpdateAsync(missionId, userId, ct);
            await missions.RequireSharedRoomAsync(missionId, ct);
        }
        var session = await OwnerAsync(id, userId, ct); Incarnation(session, body.IncarnationId);
        var signature = RoomSignature(session, device, body);
        var before = await TerminalAccess.PeopleAsync(db, id, ct);
        if (session.MissionId != body.MissionId) await db.SessionMembers.Where(x => x.SessionId == id).ExecuteDeleteAsync(ct);
        session.MissionId = body.MissionId;
        session.RoomSequence = body.Choice.Sequence; session.RoomDeviceId = device.Id; session.RoomSignature = signature;
        session.AuthorizationRevision = checked(session.AuthorizationRevision + 1);
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        await TerminalAccess.RevokeAsync(db, connections, [session], ct);
        var described = await DescribeAsync(session, userId, ct);
        connections.Notify(before.Concat(described.SharedWith).Append(userId), "sessions");
        return described;
    }

    public async Task<SessionDto> ShareAsync(Guid id, Guid userId, string deviceId, ShareSession body, CancellationToken ct)
    {
        var session = await HostAsync(id, userId, deviceId, ct); Match(session, body.IncarnationId, body.ExpectedRevision);
        if (session.MissionId is not null) throw ApiException.Conflict("This terminal is shared with its room. Remove it from the room to share it individually.");
        if (body.UserIds is null || body.UserIds.Length > Limits.MaxSessionMembers || body.UserIds.Any(x => x == Guid.Empty || x == userId))
            throw ApiException.Invalid($"Choose up to {Limits.MaxSessionMembers} friends, excluding yourself.");
        var selected = body.UserIds.ToHashSet();
        var most = (await plans.LimitsAsync(db, userId, ct)).SharePeople;
        if (selected.Count > most) throw ApiException.Forbidden($"You can share a terminal with at most {Limits.Count(most, "person", "people")}.");
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        if (!selected.SetEquals(await LockFriendshipsAsync(userId, [.. selected], ct))) throw ApiException.Forbidden("Share a terminal only with accepted friends.");
        var existing = await db.SessionMembers.Where(x => x.SessionId == id).ToListAsync(ct);
        if (selected.SetEquals(existing.Select(x => x.UserId))) return await DescribeAsync(session, userId, ct);
        var affected = existing.Select(x => x.UserId).Union(selected).Append(userId).ToArray();
        db.SessionMembers.RemoveRange(existing.Where(x => !selected.Contains(x.UserId)));
        foreach (var recipient in selected.Except(existing.Select(x => x.UserId))) db.SessionMembers.Add(new SessionMember { SessionId = id, UserId = recipient });
        session.AuthorizationRevision = checked(session.AuthorizationRevision + 1);
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        await TerminalAccess.RevokeAsync(db, connections, [session], ct);
        connections.Notify(affected, "sessions");
        return await DescribeAsync(session, userId, ct);
    }

    public async Task LeaveAsync(Guid id, Guid userId, LeaveSession body, CancellationToken ct)
    {
        var session = await AuthorizedAsync(id, userId, ct); Match(session, body.IncarnationId, body.ExpectedRevision);
        if (session.OwnerUserId == userId) throw ApiException.Invalid("You cannot leave your own terminal.");
        if (session.MissionId is not null) throw ApiException.Conflict("Leave the room to leave its shared terminals.");
        var member = await db.SessionMembers.SingleOrDefaultAsync(x => x.SessionId == id && x.UserId == userId, ct) ?? throw ApiException.Missing();
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        db.SessionMembers.Remove(member);
        session.AuthorizationRevision = checked(session.AuthorizationRevision + 1);
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        await TerminalAccess.RevokeAsync(db, connections, [session], ct);
        connections.Notify((await TerminalAccess.PeopleAsync(db, id, ct)).Append(userId), "sessions");
    }

    public async Task EndAsync(Guid id, Guid userId, string deviceId, Guid incarnationId, CancellationToken ct)
    {
        var session = await db.Sessions.SingleOrDefaultAsync(x => x.Id == id, ct);
        if (session is null) return;
        if (session.OwnerUserId != userId || session.HostDeviceId != deviceId) throw ApiException.Forbidden();
        Incarnation(session, incarnationId);
        if (session.Ended) return;
        End(session, clock.GetUtcNow());
        await db.SaveChangesAsync(ct);
        connections.RemoveSession(id);
        connections.Notify(await TerminalAccess.PeopleAsync(db, id, ct), "sessions");
    }

    public async Task HideAsync(Guid id, Guid userId, HideSession body, CancellationToken ct)
    {
        var session = await AuthorizedAsync(id, userId, ct); Incarnation(session, body.IncarnationId);
        var hidden = db.HiddenSessions.Where(x => x.SessionId == id && x.UserId == userId);
        if (connections.Live(id).Online || await hidden.AnyAsync(ct)) return;
        db.HiddenSessions.Add(new HiddenSession { UserId = userId, SessionId = id });
        await db.SaveChangesAsync(ct);
        // A host that came back before the write showed the terminal without this mark.
        if (connections.Live(id).Online) await hidden.ExecuteDeleteAsync(ct);
        else connections.Notify([userId], "sessions");
    }

    public async Task ShowAgainAsync(Guid id, CancellationToken ct)
    {
        var hidden = db.HiddenSessions.Where(x => x.SessionId == id);
        var users = await hidden.Select(x => x.UserId).ToListAsync(ct);
        if (users.Count == 0) return;
        await hidden.ExecuteDeleteAsync(ct);
        connections.Notify(users, "sessions");
    }

    public async Task RequestCloseAsync(Guid id, Guid userId, ApprovedDevice device, RequestClose body, CancellationToken ct)
    {
        var session = await AuthorizedAsync(id, userId, ct); Incarnation(session, body.IncarnationId);
        if (body.DeviceId != device.Id) throw ApiException.Forbidden("The device that asks to close a terminal must sign it.");
        var signature = Limits.Base64(body.Signature, "Close signature", IdentityWireFormat.MlDsa65SignatureLength);
        var signed = Proofs.TerminalClose(session.OwnerUserId, session.Id, session.IncarnationId, userId, device.Id);
        if (!SignatureVerifier.Verify(device.SigningPublicKey, signed, signature)) throw ApiException.Forbidden("Invalid close signature.");
        var asked = await db.Sessions.Where(x => x.Id == id && x.CloseSignature == null).ExecuteUpdateAsync(x => x
            .SetProperty(s => s.CloseUserId, userId).SetProperty(s => s.CloseDeviceId, device.Id).SetProperty(s => s.CloseSignature, signature), ct);
        if (asked == 0) return;
        connections.Notify(await TerminalAccess.PeopleAsync(db, id, ct), "sessions");
        connections.TellHost(id);
    }

    public async Task RefuseCloseAsync(Guid id, Guid userId, string deviceId, Guid incarnationId, CancellationToken ct)
    {
        var session = await HostAsync(id, userId, deviceId, ct); Incarnation(session, incarnationId);
        if (session.CloseSignature is null) return;
        session.CloseUserId = null; session.CloseDeviceId = null; session.CloseSignature = null;
        await db.SaveChangesAsync(ct);
        connections.Notify(await TerminalAccess.PeopleAsync(db, id, ct), "sessions");
    }

    // FOR SHARE: an unfriend at the same time waits, and then removes this share too.
    private async Task<List<Guid>> LockFriendshipsAsync(Guid userId, Guid[] people, CancellationToken ct) =>
        (await db.Friendships.FromSqlInterpolated($"""
            SELECT * FROM friendships WHERE "Accepted"
              AND (("FirstUserId" = {userId} AND "SecondUserId" = ANY({people})) OR ("SecondUserId" = {userId} AND "FirstUserId" = ANY({people})))
            FOR SHARE
            """).AsNoTracking().ToListAsync(ct)).Select(x => x.FirstUserId == userId ? x.SecondUserId : x.FirstUserId).ToList();

    private async Task<SessionDto> DescribeAndNotifyAsync(Session session, CancellationToken ct)
    {
        var described = await DescribeAsync(session, session.OwnerUserId, ct);
        connections.Notify(described.SharedWith.Append(described.OwnerUserId), "sessions");
        return described;
    }

    private static byte[] RoomSignature(Session session, ApprovedDevice device, AttachSession body)
    {
        var choice = body.Choice;
        if (choice.DeviceId != device.Id) throw ApiException.Forbidden("The device that sends the room must sign it.");
        if (choice.Sequence <= session.RoomSequence) throw ApiException.Conflict("The room of this terminal changed; refresh before retrying.");
        var signature = Limits.Base64(choice.Signature, "Room signature", IdentityWireFormat.MlDsa65SignatureLength);
        var signed = Proofs.TerminalRoom(session.OwnerUserId, session.Id, session.IncarnationId, body.MissionId, choice.Sequence, device.Id);
        return SignatureVerifier.Verify(device.SigningPublicKey, signed, signature) ? signature : throw ApiException.Forbidden("Invalid room signature.");
    }

    internal static void LeaveRoom(Session session)
    {
        session.MissionId = null;
        session.RoomDeviceId = null;
        session.RoomSignature = null;
    }

    internal static void End(Session session, DateTimeOffset now)
    {
        session.Ended = true;
        session.ExpiresAt = now + EndedRecordLife;
    }

    internal static void Incarnation(Session session, Guid expected)
    { if (expected == Guid.Empty || session.IncarnationId != expected) throw ApiException.Conflict("This terminal was replaced."); }
    internal static void Match(Session session, Guid incarnation, long revision)
    { Incarnation(session, incarnation); if (session.AuthorizationRevision != revision) throw ApiException.Conflict("The sharing of this terminal changed; refresh before retrying."); }

    public sealed record SessionDto(Guid Id, Guid IncarnationId, string Name, Guid OwnerUserId, string OwnerName,
        string HostDeviceId, string HostName, Guid? MissionId, string? MissionName, Guid[] SharedWith, long AuthorizationRevision, bool HostOnline,
        TerminalStatus? Status, long RoomSequence, RoomChoice? RoomChoice, bool Hidden, CloseRequest? CloseRequest);
    public sealed record RoomChoice(string DeviceId, string Signature);
    public sealed record CloseRequest(Guid UserId, string DeviceId, string Signature);
    public sealed record CreateSession(Guid Id, Guid IncarnationId, string Name, string HostDeviceId, string HostName, Guid? MissionId);
    public sealed record RenameSession(Guid IncarnationId, long ExpectedRevision, string Name);
    public sealed record AttachSession(Guid IncarnationId, Guid? MissionId, RoomChoiceWrite Choice);
    public sealed record RoomChoiceWrite(string DeviceId, long Sequence, string Signature);
    public sealed record ShareSession(Guid IncarnationId, long ExpectedRevision, Guid[] UserIds);
    public sealed record LeaveSession(Guid IncarnationId, long ExpectedRevision);
    public sealed record HideSession(Guid IncarnationId);
    public sealed record RequestClose(Guid IncarnationId, string DeviceId, string Signature);
}
