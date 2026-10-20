using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Friends;
using Kodosi.Plans;
using Kodosi.Sessions;
using Kodosi.TerminalConnections;
using Microsoft.AspNetCore.Mvc;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Missions;

public sealed partial class MissionService(KodosiDbContext db, ConnectionDirectory connections, FriendService friends, TimeProvider clock, DeviceService devices, PlanCatalog plans)
{
    private async Task<Mission> RequireMemberAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var mission = await db.Missions.SingleOrDefaultAsync(x => x.Id == id, ct) ?? throw ApiException.Missing();
        return await IsMemberAsync(id, userId, ct) ? mission : throw ApiException.Missing();
    }

    internal async Task<Mission> RequireMemberForUpdateAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var mission = await LockAsync(id, ct);
        return await IsMemberAsync(id, userId, ct) ? mission : throw ApiException.Missing();
    }

    private Task<bool> IsMemberAsync(Guid id, Guid userId, CancellationToken ct) =>
        RoomMembership.People(db).AnyAsync(x => x.MissionId == id && x.UserId == userId, ct);

    private async Task<Mission> LockAsync(Guid id, CancellationToken ct)
    {
        if (db.Database.CurrentTransaction is null) throw new InvalidOperationException("A room change requires a transaction.");
        return await db.Missions.FromSqlInterpolated($"SELECT * FROM missions WHERE \"Id\" = {id} FOR UPDATE").SingleOrDefaultAsync(ct)
            ?? throw ApiException.Missing();
    }

    internal Task<List<Guid>> MemberIdsAsync(Guid id, CancellationToken ct, bool invited = false) =>
        RoomMembership.PeopleOf(db, id, invited).Distinct().ToListAsync(ct);

    private IQueryable<Session> TerminalsOf(Guid id) => db.Sessions.Where(x => x.MissionId == id && !x.Ended);

    private async Task<HashSet<Guid>> TerminalPeopleAsync(Guid id, CancellationToken ct)
    {
        var terminals = TerminalsOf(id).Select(x => x.Id);
        return (await TerminalAccess.People(db).Where(x => terminals.Contains(x.SessionId)).Select(x => x.UserId).Distinct().ToListAsync(ct)).ToHashSet();
    }

    public async Task<object> ListAsync(Guid userId, CancellationToken ct)
    {
        var rooms = RoomMembership.RoomsOf(db, userId);
        var missions = await db.Missions.AsNoTracking().Where(x => rooms.Contains(x.Id) && !x.Dosi)
            .OrderBy(x => x.Name).ThenBy(x => x.Id).Take(Limits.MaxVisibleMissions + 1).Select(x => new { x.Id, x.Name, x.OwnerUserId }).ToListAsync(ct);
        var dosi = await db.Missions.AsNoTracking().Where(x => x.OwnerUserId == userId && x.Dosi).Select(x => (Guid?)x.Id).SingleOrDefaultAsync(ct);
        var invitations = await (from i in db.MissionInvitations.AsNoTracking()
                                 join r in db.Missions on i.MissionId equals r.Id
                                 join u in db.Users on i.InviterUserId equals u.Id
                                 where i.UserId == userId
                                 orderby i.CreatedAt descending
                                 select new { i.Id, i.MissionId, missionName = r.Name, inviterName = u.DisplayName, i.CreatedAt }).Take(Limits.MaxPendingMissionInvitations + 1).ToListAsync(ct);
        return new
        {
            missions = missions.Take(Limits.MaxVisibleMissions).ToArray(),
            invitations = invitations.Take(Limits.MaxPendingMissionInvitations).ToArray(),
            missionsTruncated = missions.Count > Limits.MaxVisibleMissions,
            invitationsTruncated = invitations.Count > Limits.MaxPendingMissionInvitations,
            dosi
        };
    }

    public async Task<object> OpenAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var mission = await RequireMemberAsync(id, userId, ct);
        var people = RoomMembership.PeopleOf(db, id);
        var members = await db.Users.AsNoTracking().Where(u => people.Contains(u.Id))
            .OrderBy(u => u.Handle).Select(u => new { userId = u.Id, u.Handle, u.DisplayName, isOwner = u.Id == mission.OwnerUserId }).ToListAsync(ct);
        return new { mission = new { mission.Id, mission.Name, mission.OwnerUserId }, members };
    }

    public async Task CreateAsync(Guid userId, CreateMission body, CancellationToken ct)
    {
        Limits.Id(body.Id, "Room ID");
        var name = Limits.Text(body.Name, "Mission name", 128);
        var existing = await db.Missions.SingleOrDefaultAsync(x => x.Id == body.Id, ct);
        if (existing is not null)
        {
            if (existing.OwnerUserId != userId || existing.Name != name || existing.Dosi != body.Dosi) throw ApiException.Conflict("This room ID is already in use.");
            return;
        }
        if (body.Dosi)
        {
            if (await db.Missions.AnyAsync(x => x.OwnerUserId == userId && x.Dosi, ct))
                throw ApiException.Conflict("Dosi already has a conversation.");
        }
        else
        {
            var owned = await db.Missions.CountAsync(x => x.OwnerUserId == userId && !x.Dosi, ct);
            if (owned >= (await plans.LimitsAsync(db, userId, ct)).OwnedRooms)
                throw ApiException.Forbidden($"You own {Limits.Count(owned, "room", "rooms")}, the most that you can own. Delete a room to make a new one.");
            await RequireMissionCapacityAsync(userId, ct);
        }
        db.Missions.Add(new Mission { Id = body.Id, Name = name, OwnerUserId = userId, Dosi = body.Dosi });
        await db.SaveChangesAsync(ct); connections.Notify([userId], "missions", body.Id);
    }

    private static void RequireRoom(Mission mission)
    {
        if (mission.Dosi) throw ApiException.Forbidden("Dosi's conversation belongs to you alone.");
    }

    public async Task RenameAsync(Guid id, Guid userId, string name, CancellationToken ct)
    {
        var mission = await RequireMemberAsync(id, userId, ct);
        if (mission.OwnerUserId != userId) throw ApiException.Forbidden();
        RequireRoom(mission);
        mission.Name = Limits.Text(name, "Mission name", 128);
        await db.SaveChangesAsync(ct);
        await NotifyAsync(id, ct);
        connections.Notify(await TerminalPeopleAsync(id, ct), "sessions");
    }

    public async Task DeleteAsync(Guid id, Guid userId, CancellationToken ct)
    {
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        if (!await db.Missions.AnyAsync(x => x.Id == id, ct)) return;
        var mission = await RequireMemberForUpdateAsync(id, userId, ct);
        if (mission.OwnerUserId != userId) throw ApiException.Forbidden();
        var people = await MemberIdsAsync(id, ct, invited: true);
        var viewers = await TerminalPeopleAsync(id, ct);
        var terminals = await TerminalsOf(id).ToListAsync(ct);
        foreach (var terminal in terminals)
        {
            SessionService.LeaveRoom(terminal);
            terminal.AuthorizationRevision = checked(terminal.AuthorizationRevision + 1);
        }
        db.Missions.Remove(mission); await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        await TerminalAccess.RevokeAsync(db, connections, terminals, ct);
        connections.Notify(people, "missions", id);
        connections.Notify(viewers, "sessions");
    }

    public async Task InviteAsync(Guid id, Guid userId, Invite body, CancellationToken ct)
    {
        var keys = await PrepareKeysAsync(userId, body.Keys, ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var mission = await RequireMemberForUpdateAsync(id, userId, ct);
        if (mission.OwnerUserId != userId) throw ApiException.Forbidden();
        RequireRoom(mission);
        if (!await friends.AreFriendsAsync(userId, body.UserId, ct)) throw ApiException.Forbidden("Invite an accepted friend.");
        if (await db.MissionMembers.AnyAsync(x => x.MissionId == id && x.UserId == body.UserId, ct)) return;
        if (await db.MissionInvitations.AnyAsync(x => x.MissionId == id && x.UserId == body.UserId, ct)) return;
        if (await db.MissionInvitations.CountAsync(x => x.UserId == body.UserId, ct) >= Limits.MaxPendingMissionInvitations)
            throw ApiException.Conflict("This person has too many open room invitations.");
        await RequireMissionCapacityAsync(body.UserId, ct);
        var people = await RoomMembership.PeopleOf(db, id, invited: true).CountAsync(ct);
        var most = (await plans.LimitsAsync(db, mission.OwnerUserId, ct)).RoomPeople;
        if (people >= most) throw ApiException.Forbidden($"This room can have at most {Limits.Count(most, "person", "people")}, with its open invitations.");
        db.MissionInvitations.Add(new MissionInvitation { Id = Limits.Id(body.Id, "Invitation ID"), MissionId = id, UserId = body.UserId, InviterUserId = userId, CreatedAt = clock.GetUtcNow() });
        await db.SaveChangesAsync(ct);
        await AppendKeysAsync(mission, keys, ct);
        await transaction.CommitAsync(ct);
        connections.Notify([body.UserId], "missions", id);
        await NotifyAsync(id, ct);
    }

    public async Task ResolveInvitationAsync(Guid id, Guid userId, bool accept, CancellationToken ct, RoomKeyWrite? keys = null)
    {
        var leaving = accept ? null : await PrepareKeysAsync(userId, keys, ct);
        var roomId = await db.MissionInvitations.Where(x => x.Id == id && x.UserId == userId).Select(x => x.MissionId).SingleOrDefaultAsync(ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var mission = await LockAsync(roomId, ct);
        var invitation = await db.MissionInvitations.SingleOrDefaultAsync(x => x.Id == id && x.UserId == userId, ct) ?? throw ApiException.Missing();
        if (accept)
        {
            if (!await friends.AreFriendsAsync(mission.OwnerUserId, userId, ct)) throw ApiException.Forbidden("This invitation is no longer available.");
            if (!await db.MissionMembers.AnyAsync(x => x.MissionId == mission.Id && x.UserId == userId, ct))
            {
                await RequireMissionCapacityAsync(userId, ct);
                db.MissionMembers.Add(new MissionMember { MissionId = mission.Id, UserId = userId });
            }
        }
        db.MissionInvitations.Remove(invitation); await db.SaveChangesAsync(ct);
        if (!accept) await AppendKeysAsync(mission, leaving, ct);
        await TerminalsOf(mission.Id).ExecuteUpdateAsync(set => set.SetProperty(x => x.AuthorizationRevision, x => x.AuthorizationRevision + 1), ct);
        await transaction.CommitAsync(ct);
        await NotifyAsync(mission.Id, ct); connections.Notify([userId], "missions", mission.Id);
        await TerminalAccess.RevokeAsync(db, connections, await TerminalsOf(mission.Id).AsNoTracking().ToListAsync(ct), ct);
        connections.Notify((await TerminalPeopleAsync(mission.Id, ct)).Append(userId), "sessions");
    }

    public async Task RemoveMemberAsync(Guid id, Guid actor, Guid userId, CancellationToken ct, RoomKeyWrite? keys = null)
    {
        var prepared = await PrepareKeysAsync(actor, keys, ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var mission = await RequireMemberForUpdateAsync(id, actor, ct);
        if (userId == mission.OwnerUserId) throw ApiException.Invalid("The owner cannot leave the room. Delete it instead.");
        if (actor != userId && mission.OwnerUserId != actor) throw ApiException.Forbidden();
        var viewers = await TerminalPeopleAsync(id, ct);
        var terminals = await TerminalsOf(id).ToListAsync(ct);
        await db.MissionMembers.Where(x => x.MissionId == id && x.UserId == userId).ExecuteDeleteAsync(ct);
        await db.MissionInvitations.Where(x => x.MissionId == id && x.UserId == userId).ExecuteDeleteAsync(ct);
        foreach (var terminal in terminals)
        {
            if (terminal.OwnerUserId == userId) SessionService.LeaveRoom(terminal);
            terminal.AuthorizationRevision = checked(terminal.AuthorizationRevision + 1);
        }
        await db.SaveChangesAsync(ct);
        await AppendKeysAsync(mission, prepared, ct);
        await transaction.CommitAsync(ct);
        await TerminalAccess.RevokeAsync(db, connections, terminals, ct);
        await NotifyAsync(id, ct); connections.Notify([userId], "missions", id);
        connections.Notify(viewers.Append(userId), "sessions");
    }

    internal async Task RequireSharedRoomAsync(Guid id, CancellationToken ct)
    {
        if (await db.Missions.AnyAsync(x => x.Id == id && x.Dosi, ct))
            throw ApiException.Invalid("Terminals do not go into Dosi's conversation.");
    }

    private async Task RequireMissionCapacityAsync(Guid userId, CancellationToken ct)
    {
        var rooms = RoomMembership.RoomsOf(db, userId);
        if (await db.Missions.CountAsync(x => rooms.Contains(x.Id) && !x.Dosi, ct) >= Limits.MaxVisibleMissions)
            throw ApiException.Conflict("Leave a room before you join or make another.");
    }

    private async Task NotifyAsync(Guid id, CancellationToken ct) => connections.Notify(await MemberIdsAsync(id, ct), "missions", id);

    public sealed record CreateMission(Guid Id, string Name, bool Dosi = false);
    public sealed record RenameMission(string Name);
    public sealed record Invite(Guid Id, Guid UserId, RoomKeyWrite? Keys = null);
}

internal static class MissionEndpoints
{
    public static void MapMissions(this IEndpointRouteBuilder app)
    {
        app.MapPut("/api/me/room-key", async (MissionService.RecipientKeyWrite body, DeviceCaller caller, MissionService missions, CancellationToken ct) =>
        {
            await missions.RegisterRoomKeyAsync(caller.User.Id, caller.Device, body, ct);
            return Results.NoContent();
        }).RequireAuthorization();
        app.MapGet("/api/users/{userId:guid}/room-keys", async (Guid userId, DeviceCaller caller, MissionService missions, CancellationToken ct) =>
            Results.Ok(await missions.RecipientKeysAsync(caller.User.Id, userId, ct))).RequireAuthorization();
        var group = app.MapGroup("/api/missions").RequireAuthorization();
        group.MapGet("/{id:guid}/keys", async (Guid id, long? afterVersion, DeviceCaller caller, MissionService missions, CancellationToken ct) =>
            Results.Ok(await missions.KeyHistoryAsync(id, caller.User.Id, afterVersion ?? 0, ct)));
        group.MapPut("/{id:guid}/keys", async (Guid id, MissionService.RoomKeyWrite body, DeviceCaller caller, MissionService missions, CancellationToken ct) =>
        {
            await missions.StoreKeysAsync(id, caller.User.Id, body, ct);
            return Results.NoContent();
        });
        group.MapGet("/{id:guid}/content", async (Guid id, string? kind, long? after, long? before, int? limit, DeviceCaller caller, MissionService missions,
            CancellationToken ct) => Results.Ok(await missions.ContentAsync(id, caller.User.Id, kind, after ?? 0, before, limit, ct)));
        group.MapPut("/{id:guid}/content/{itemId:guid}", async (Guid id, Guid itemId, MissionService.ContentWrite body, DeviceCaller caller, MissionService missions,
            CancellationToken ct) => Results.Ok(await missions.PutContentAsync(id, itemId, caller.User.Id, body, ct, caller.Device)));
        group.MapGet("/", async (DeviceCaller caller, MissionService missions, CancellationToken ct) =>
            Results.Ok(await missions.ListAsync(caller.User.Id, ct)));
        group.MapPost("/", async (MissionService.CreateMission body, DeviceCaller caller, MissionService missions, CancellationToken ct) =>
        {
            await missions.CreateAsync(caller.User.Id, body, ct);
            return Results.NoContent();
        });
        group.MapGet("/{id:guid}", async (Guid id, DeviceCaller caller, MissionService missions, CancellationToken ct) =>
            Results.Ok(await missions.OpenAsync(id, caller.User.Id, ct)));
        group.MapPatch("/{id:guid}", async (Guid id, MissionService.RenameMission body, DeviceCaller caller, MissionService missions, CancellationToken ct) =>
        {
            await missions.RenameAsync(id, caller.User.Id, body.Name, ct);
            return Results.NoContent();
        });
        group.MapDelete("/{id:guid}", async (Guid id, DeviceCaller caller, MissionService missions, CancellationToken ct) =>
        {
            await missions.DeleteAsync(id, caller.User.Id, ct);
            return Results.NoContent();
        });
        group.MapPost("/{id:guid}/invitations", async (Guid id, MissionService.Invite body, DeviceCaller caller, MissionService missions, CancellationToken ct) =>
        {
            await missions.InviteAsync(id, caller.User.Id, body, ct);
            return Results.NoContent();
        });
        foreach (var accept in new[] { true, false })
            group.MapPost($"/invitations/{{id:guid}}/{(accept ? "accept" : "reject")}", async (Guid id, [FromBody] MissionService.MembershipWrite? body,
                DeviceCaller caller, MissionService missions, CancellationToken ct) =>
            {
                await missions.ResolveInvitationAsync(id, caller.User.Id, accept, ct, body?.Keys);
                return Results.NoContent();
            });
        group.MapDelete("/{id:guid}/members/{userId:guid}", async (Guid id, Guid userId, [FromBody] MissionService.MembershipWrite? body, DeviceCaller caller,
            MissionService missions, CancellationToken ct) =>
        {
            await missions.RemoveMemberAsync(id, caller.User.Id, userId, ct, body?.Keys);
            return Results.NoContent();
        });
    }
}
