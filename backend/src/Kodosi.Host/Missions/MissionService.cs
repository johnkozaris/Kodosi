using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Friends;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Missions;

public sealed partial class MissionService(KodosiDbContext db, ConnectionDirectory connections, FriendService friends, TimeProvider clock, DeviceService devices, Kodosi.Security.SignatureVerifier signatures)
{
    public async Task<Mission> RequireMemberAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var mission = await db.Missions.SingleOrDefaultAsync(x => x.Id == id, ct) ?? throw ApiException.Missing();
        if (mission.OwnerUserId != userId && !await db.MissionMembers.AnyAsync(x => x.MissionId == id && x.UserId == userId, ct))
            throw ApiException.Missing();
        return mission;
    }

    internal async Task<Mission> RequireMemberForUpdateAsync(Guid id, Guid userId, CancellationToken ct)
    {
        if (db.Database.CurrentTransaction is null) throw new InvalidOperationException("A room change requires a transaction.");
        _ = await db.Missions.FromSqlInterpolated($"SELECT * FROM missions WHERE \"Id\" = {id} FOR UPDATE").SingleOrDefaultAsync(ct)
            ?? throw ApiException.Missing();
        return await RequireMemberAsync(id, userId, ct);
    }

    internal async Task<List<Guid>> MemberIdsAsync(Guid id, CancellationToken ct)
    {
        var members = await db.MissionMembers.Where(x => x.MissionId == id).Select(x => x.UserId).ToListAsync(ct);
        var owner = await db.Missions.Where(x => x.Id == id).Select(x => (Guid?)x.OwnerUserId).SingleOrDefaultAsync(ct);
        if (owner is { } user) members.Add(user);
        return members.Distinct().ToList();
    }

    public async Task<object> ListAsync(Guid userId, CancellationToken ct)
    {
        var missions = await db.Missions.AsNoTracking().Where(r => r.OwnerUserId == userId || db.MissionMembers.Any(m => m.MissionId == r.Id && m.UserId == userId))
            .OrderBy(r => r.Name).ThenBy(r => r.Id).Take(Limits.MaxVisibleMissions + 1).Select(r => new { r.Id, r.Name, r.OwnerUserId }).ToListAsync(ct);
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
            invitationsTruncated = invitations.Count > Limits.MaxPendingMissionInvitations
        };
    }

    public async Task<object> OpenAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var mission = await RequireMemberAsync(id, userId, ct);
        var members = await db.Users.AsNoTracking().Where(u => u.Id == mission.OwnerUserId || db.MissionMembers.Any(m => m.MissionId == id && m.UserId == u.Id))
            .OrderBy(u => u.Handle).Select(u => new { userId = u.Id, u.Handle, u.DisplayName, isOwner = u.Id == mission.OwnerUserId }).ToListAsync(ct);
        return new { mission = new { mission.Id, mission.Name, mission.OwnerUserId }, members };
    }

    public async Task<object> CreateAsync(Guid userId, CreateMission body, CancellationToken ct)
    {
        Limits.Id(body.Id, "Mission ID");
        var name = Limits.Text(body.Name, "Mission name", 128);
        var existing = await db.Missions.SingleOrDefaultAsync(x => x.Id == body.Id, ct);
        if (existing is not null)
        {
            if (existing.OwnerUserId != userId || existing.Name != name) throw ApiException.Conflict("Mission ID was reused.");
            return new { existing.Id, existing.Name, existing.OwnerUserId };
        }
        if (await db.Missions.CountAsync(x => x.OwnerUserId == userId, ct) >= Limits.MaxMissionsPerUser)
            throw ApiException.Conflict("Too many Missions.");
        await RequireMissionCapacityAsync(userId, ct);
        var mission = new Mission { Id = body.Id, Name = name, OwnerUserId = userId };
        db.Missions.Add(mission); await db.SaveChangesAsync(ct); connections.Notify(userId, "missions");
        return new { mission.Id, mission.Name, mission.OwnerUserId };
    }

    public async Task<object> RenameAsync(Guid id, Guid userId, string name, CancellationToken ct)
    {
        var mission = await RequireMemberAsync(id, userId, ct);
        if (mission.OwnerUserId != userId) throw ApiException.Forbidden();
        mission.Name = Limits.Text(name, "Mission name", 128); await db.SaveChangesAsync(ct); await NotifyAsync(mission, ct);
        foreach (var viewer in await AffectedSessionUsers(id, null, ct)) connections.Notify(viewer, "sessions");
        return new { mission.Id, mission.Name, mission.OwnerUserId };
    }

    public async Task DeleteAsync(Guid id, Guid userId, CancellationToken ct)
    {
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        if (!await db.Missions.AnyAsync(x => x.Id == id, ct)) return;
        var mission = await RequireMemberForUpdateAsync(id, userId, ct);
        if (mission.OwnerUserId != userId) throw ApiException.Forbidden();
        var users = await MemberIdsAsync(mission, ct);
        var invitees = await db.MissionInvitations.Where(x => x.MissionId == id).Select(x => x.UserId).ToArrayAsync(ct);
        var viewers = await AffectedSessionUsers(id, null, ct);
        var terminals = await db.Sessions.Where(x => x.MissionId == id && !x.Ended).ToListAsync(ct);
        foreach (var terminal in terminals)
        {
            terminal.MissionId = null;
            terminal.AuthorizationRevision = checked(terminal.AuthorizationRevision + 1);
        }
        db.Missions.Remove(mission); await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        foreach (var terminal in terminals) connections.Revoke(terminal, (viewer, _) => viewer == terminal.OwnerUserId);
        foreach (var member in users.Concat(invitees).Distinct()) connections.Notify(member, "missions");
        foreach (var viewer in viewers.Concat(users).Distinct()) connections.Notify(viewer, "sessions");
    }

    public async Task InviteAsync(Guid id, Guid userId, Invite body, CancellationToken ct)
    {
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var mission = await RequireMemberForUpdateAsync(id, userId, ct);
        if (mission.OwnerUserId != userId) throw ApiException.Forbidden();
        if (!await friends.AreFriendsAsync(userId, body.UserId, ct)) throw ApiException.Forbidden("Invite an accepted friend.");
        if (await db.MissionMembers.AnyAsync(x => x.MissionId == id && x.UserId == body.UserId, ct)) return;
        if (await db.MissionInvitations.AnyAsync(x => x.MissionId == id && x.UserId == body.UserId, ct)) return;
        if (await db.MissionInvitations.CountAsync(x => x.UserId == body.UserId, ct) >= Limits.MaxPendingMissionInvitations)
            throw ApiException.Conflict("This person has too many pending Mission invitations.");
        await RequireMissionCapacityAsync(body.UserId, ct);
        if (await db.MissionMembers.CountAsync(x => x.MissionId == id, ct) + await db.MissionInvitations.CountAsync(x => x.MissionId == id, ct) >= Limits.MaxMissionMembers)
            throw ApiException.Conflict("This Mission has reached its member limit.");
        db.MissionInvitations.Add(new MissionInvitation { Id = Limits.Id(body.Id, "Invitation ID"), MissionId = id, UserId = body.UserId, InviterUserId = userId, CreatedAt = clock.GetUtcNow() });
        await db.SaveChangesAsync(ct);
        await AppendKeysAsync(mission, userId, body.Keys, ct);
        await transaction.CommitAsync(ct);
        connections.Notify(body.UserId, "missions");
        await NotifyAsync(mission, ct);
    }

    public async Task ResolveInvitationAsync(Guid id, Guid userId, bool accept, CancellationToken ct, RoomKeyWrite? keys = null)
    {
        var roomId = await db.MissionInvitations.Where(x => x.Id == id && x.UserId == userId).Select(x => x.MissionId).SingleOrDefaultAsync(ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var mission = await db.Missions.FromSqlInterpolated($"SELECT * FROM missions WHERE \"Id\" = {roomId} FOR UPDATE").SingleOrDefaultAsync(ct) ?? throw ApiException.Missing();
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
        if (!accept) await AppendKeysAsync(mission, userId, keys, ct);
        await RefreshTerminalAccessAsync(mission.Id, ct);
        await transaction.CommitAsync(ct);
        await NotifyAsync(mission, ct); connections.Notify(userId, "missions");
        await NotifyTerminalUsersAsync(mission.Id, [userId], ct);
    }

    public async Task RemoveMemberAsync(Guid id, Guid actor, Guid userId, CancellationToken ct, RoomKeyWrite? keys = null)
    {
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var mission = await RequireMemberForUpdateAsync(id, actor, ct);
        if (userId == mission.OwnerUserId) throw ApiException.Invalid("The owner must delete the Mission instead of leaving.");
        if (actor != userId && mission.OwnerUserId != actor) throw ApiException.Forbidden();
        var viewers = await AffectedSessionUsers(id, userId, ct);
        var terminals = await db.Sessions.Where(x => x.MissionId == id && !x.Ended).ToListAsync(ct);
        await db.MissionMembers.Where(x => x.MissionId == id && x.UserId == userId).ExecuteDeleteAsync(ct);
        await db.MissionInvitations.Where(x => x.MissionId == id && x.UserId == userId).ExecuteDeleteAsync(ct);
        foreach (var terminal in terminals)
        {
            if (terminal.OwnerUserId == userId) terminal.MissionId = null;
            terminal.AuthorizationRevision = checked(terminal.AuthorizationRevision + 1);
        }
        await db.SaveChangesAsync(ct);
        await AppendKeysAsync(mission, actor, keys, ct);
        await transaction.CommitAsync(ct);
        var members = (await MemberIdsAsync(id, ct)).ToHashSet();
        foreach (var terminal in terminals)
            connections.Revoke(terminal, (viewer, _) => viewer == terminal.OwnerUserId || (terminal.MissionId == id && members.Contains(viewer)));
        await NotifyAsync(mission, ct); connections.Notify(userId, "missions");
        foreach (var viewer in viewers.Append(userId).Concat(members).Distinct()) connections.Notify(viewer, "sessions");
    }

    private async Task RequireMissionCapacityAsync(Guid userId, CancellationToken ct)
    {
        if (await db.Missions.CountAsync(r => r.OwnerUserId == userId || db.MissionMembers.Any(m => m.MissionId == r.Id && m.UserId == userId), ct) >= Limits.MaxVisibleMissions)
            throw ApiException.Conflict("Leave a Mission before joining or creating another.");
    }

    private async Task RefreshTerminalAccessAsync(Guid id, CancellationToken ct)
    {
        await db.Sessions.Where(x => x.MissionId == id && !x.Ended)
            .ExecuteUpdateAsync(set => set.SetProperty(x => x.AuthorizationRevision, x => x.AuthorizationRevision + 1), ct);
    }

    private async Task NotifyTerminalUsersAsync(Guid id, IEnumerable<Guid> extra, CancellationToken ct)
    {
        var members = (await MemberIdsAsync(id, ct)).ToHashSet();
        foreach (var terminal in await db.Sessions.AsNoTracking().Where(x => x.MissionId == id && !x.Ended).ToListAsync(ct))
            connections.Revoke(terminal, (viewer, _) => viewer == terminal.OwnerUserId || members.Contains(viewer));
        foreach (var user in members.Concat(extra).Distinct()) connections.Notify(user, "sessions");
    }

    private async Task<List<Guid>> AffectedSessionUsers(Guid missionId, Guid? owner, CancellationToken ct)
    {
        var affected = db.Sessions.Where(s => s.MissionId == missionId && (owner == null || s.OwnerUserId == owner));
        return await affected.Select(s => s.OwnerUserId)
            .Union(db.SessionMembers.Where(m => affected.Any(s => s.Id == m.SessionId)).Select(m => m.UserId))
            .Union(db.MissionMembers.Where(m => m.MissionId == missionId).Select(m => m.UserId))
            .Union(db.Missions.Where(m => m.Id == missionId).Select(m => m.OwnerUserId)).ToListAsync(ct);
    }

    private async Task<List<Guid>> MemberIdsAsync(Mission mission, CancellationToken ct)
    {
        var ids = await db.MissionMembers.Where(x => x.MissionId == mission.Id).Select(x => x.UserId).ToListAsync(ct);
        ids.Add(mission.OwnerUserId); return ids;
    }
    private async Task NotifyAsync(Mission mission, CancellationToken ct)
    {
        foreach (var id in await MemberIdsAsync(mission, ct)) connections.Notify(id, "missions");
    }
    public sealed record CreateMission(Guid Id, string Name);
    public sealed record RenameMission(string Name);
    public sealed record Invite(Guid Id, Guid UserId, RoomKeyWrite? Keys = null);
}

internal static class MissionEndpoints
{
    public static void MapMissions(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/missions").RequireAuthorization();
        app.MapPut("/api/me/room-key", async (MissionService.RecipientKeyWrite body, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        {
            var user = await users.GetAsync(ctx, ct); var device = await devices.RequireProofAsync(ctx, user.Id, ct);
            await missions.RegisterRoomKeyAsync(user.Id, device, body, ct); return Results.NoContent();
        }).RequireAuthorization();
        app.MapGet("/api/users/{userId:guid}/room-keys", async (Guid userId, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await missions.RecipientKeysAsync(actor, userId, ct)); }).RequireAuthorization();
        group.MapGet("/{id:guid}/keys", async (Guid id, long? afterVersion, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await missions.KeyHistoryAsync(id, actor, afterVersion ?? 0, ct)); });
        group.MapPut("/{id:guid}/keys", async (Guid id, MissionService.RoomKeyWrite body, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); await missions.StoreKeysAsync(id, actor, body, ct); return Results.NoContent(); });
        group.MapGet("/{id:guid}/content", async (Guid id, string? kind, long? after, long? before, int? limit, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await missions.ContentAsync(id, actor, kind, after ?? 0, before, limit, ct)); });
        group.MapPut("/{id:guid}/content/{itemId:guid}", async (Guid id, Guid itemId, MissionService.ContentWrite body, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await missions.PutContentAsync(id, itemId, actor, body, ct)); });
        group.MapGet("/", async (HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var id = await Actor(ctx, users, devices, ct); return Results.Ok(await missions.ListAsync(id, ct)); });
        group.MapPost("/", async (MissionService.CreateMission body, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var id = await Actor(ctx, users, devices, ct); return Results.Ok(await missions.CreateAsync(id, body, ct)); });
        group.MapGet("/{id:guid}", async (Guid id, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await missions.OpenAsync(id, actor, ct)); });
        group.MapPatch("/{id:guid}", async (Guid id, MissionService.RenameMission body, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await missions.RenameAsync(id, actor, body.Name, ct)); });
        group.MapDelete("/{id:guid}", async (Guid id, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); await missions.DeleteAsync(id, actor, ct); return Results.NoContent(); });
        group.MapPost("/{id:guid}/invitations", async (Guid id, MissionService.Invite body, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); await missions.InviteAsync(id, actor, body, ct); return Results.NoContent(); });
        foreach (var action in new[] { "accept", "reject" })
            group.MapPost($"/invitations/{{id:guid}}/{action}", async (Guid id, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
            { var actor = await Actor(ctx, users, devices, ct); var body = ctx.Request.ContentLength is > 0 ? await ctx.Request.ReadFromJsonAsync<MissionService.MembershipWrite>(cancellationToken: ct) : null; await missions.ResolveInvitationAsync(id, actor, action == "accept", ct, body?.Keys); return Results.NoContent(); });
        group.MapDelete("/{id:guid}/members/{userId:guid}", async (Guid id, Guid userId, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); var body = ctx.Request.ContentLength is > 0 ? await ctx.Request.ReadFromJsonAsync<MissionService.MembershipWrite>(cancellationToken: ct) : null; await missions.RemoveMemberAsync(id, actor, userId, ct, body?.Keys); return Results.NoContent(); });
    }
    private static async Task<Guid> Actor(HttpContext ctx, CurrentUser users, DeviceService devices, CancellationToken ct)
    { var user = await users.GetAsync(ctx, ct); await devices.RequireProofAsync(ctx, user.Id, ct); return user.Id; }
}
