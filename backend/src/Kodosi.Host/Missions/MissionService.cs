using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Friends;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Missions;

public sealed class MissionService(KodosiDbContext db, ConnectionDirectory connections, FriendService friends, TimeProvider clock)
{
    public async Task<Mission> RequireMemberAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var mission = await db.Missions.SingleOrDefaultAsync(x => x.Id == id, ct) ?? throw ApiException.Missing();
        if (mission.OwnerUserId != userId && !await db.MissionMembers.AnyAsync(x => x.MissionId == id && x.UserId == userId, ct))
            throw ApiException.Missing();
        return mission;
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
        var mission = await db.Missions.SingleOrDefaultAsync(x => x.Id == id, ct);
        if (mission is null) return;
        if (mission.OwnerUserId != userId) throw ApiException.Forbidden();
        var users = await MemberIdsAsync(mission, ct);
        var invitees = await db.MissionInvitations.Where(x => x.MissionId == id).Select(x => x.UserId).ToArrayAsync(ct);
        var viewers = await AffectedSessionUsers(id, null, ct);
        db.Missions.Remove(mission); await db.SaveChangesAsync(ct);
        foreach (var member in users.Concat(invitees).Distinct()) connections.Notify(member, "missions");
        foreach (var viewer in viewers.Concat(users).Distinct()) connections.Notify(viewer, "sessions");
    }

    public async Task InviteAsync(Guid id, Guid userId, Invite body, CancellationToken ct)
    {
        var mission = await RequireMemberAsync(id, userId, ct);
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
        connections.Notify(body.UserId, "missions");
    }

    public async Task ResolveInvitationAsync(Guid id, Guid userId, bool accept, CancellationToken ct)
    {
        var invitation = await db.MissionInvitations.SingleOrDefaultAsync(x => x.Id == id && x.UserId == userId, ct) ?? throw ApiException.Missing();
        var mission = await db.Missions.SingleAsync(x => x.Id == invitation.MissionId, ct);
        if (accept)
        {
            if (!await friends.AreFriendsAsync(mission.OwnerUserId, userId, ct)) throw ApiException.Forbidden("This invitation is no longer available.");
            if (!await db.MissionMembers.AnyAsync(x => x.MissionId == mission.Id && x.UserId == userId, ct))
            {
                await RequireMissionCapacityAsync(userId, ct);
                db.MissionMembers.Add(new MissionMember { MissionId = mission.Id, UserId = userId });
            }
        }
        db.MissionInvitations.Remove(invitation); await db.SaveChangesAsync(ct); await NotifyAsync(mission, ct); connections.Notify(userId, "missions");
    }

    public async Task RemoveMemberAsync(Guid id, Guid actor, Guid userId, CancellationToken ct)
    {
        var mission = await RequireMemberAsync(id, actor, ct);
        if (userId == mission.OwnerUserId) throw ApiException.Invalid("The owner must delete the Mission instead of leaving.");
        if (actor != userId && mission.OwnerUserId != actor) throw ApiException.Forbidden();
        var viewers = await AffectedSessionUsers(id, userId, ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        await db.MissionMembers.Where(x => x.MissionId == id && x.UserId == userId).ExecuteDeleteAsync(ct);
        await db.MissionInvitations.Where(x => x.MissionId == id && x.UserId == userId).ExecuteDeleteAsync(ct);
        await db.Sessions.Where(x => x.MissionId == id && x.OwnerUserId == userId).ExecuteUpdateAsync(set => set.SetProperty(x => x.MissionId, (Guid?)null), ct);
        await transaction.CommitAsync(ct);
        await NotifyAsync(mission, ct); connections.Notify(userId, "missions");
        foreach (var viewer in viewers.Append(userId).Distinct()) connections.Notify(viewer, "sessions");
    }

    private async Task RequireMissionCapacityAsync(Guid userId, CancellationToken ct)
    {
        if (await db.Missions.CountAsync(r => r.OwnerUserId == userId || db.MissionMembers.Any(m => m.MissionId == r.Id && m.UserId == userId), ct) >= Limits.MaxVisibleMissions)
            throw ApiException.Conflict("Leave a Mission before joining or creating another.");
    }

    private async Task<List<Guid>> AffectedSessionUsers(Guid missionId, Guid? owner, CancellationToken ct)
    {
        var affected = db.Sessions.Where(s => s.MissionId == missionId && (owner == null || s.OwnerUserId == owner));
        return await affected.Select(s => s.OwnerUserId)
            .Union(db.SessionMembers.Where(m => affected.Any(s => s.Id == m.SessionId)).Select(m => m.UserId)).ToListAsync(ct);
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
    public sealed record Invite(Guid Id, Guid UserId);
}

internal static class MissionEndpoints
{
    public static void MapMissions(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/missions").RequireAuthorization();
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
            { var actor = await Actor(ctx, users, devices, ct); await missions.ResolveInvitationAsync(id, actor, action == "accept", ct); return Results.NoContent(); });
        group.MapDelete("/{id:guid}/members/{userId:guid}", async (Guid id, Guid userId, HttpContext ctx, CurrentUser users, DeviceService devices, MissionService missions, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); await missions.RemoveMemberAsync(id, actor, userId, ct); return Results.NoContent(); });
    }
    private static async Task<Guid> Actor(HttpContext ctx, CurrentUser users, DeviceService devices, CancellationToken ct)
    { var user = await users.GetAsync(ctx, ct); await devices.RequireProofAsync(ctx, user.Id, ct); return user.Id; }
}
