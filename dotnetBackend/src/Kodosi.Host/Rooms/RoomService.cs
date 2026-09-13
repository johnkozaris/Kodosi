using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Friends;
using Kodosi.Realtime;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Rooms;

public sealed class RoomService(KodosiDbContext db, RelayDirectory relay, FriendService friends, TimeProvider clock)
{
    public async Task<Room> RequireMemberAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var room = await db.Rooms.SingleOrDefaultAsync(x => x.Id == id, ct) ?? throw ApiException.Missing();
        if (room.OwnerUserId != userId && !await db.RoomMembers.AnyAsync(x => x.RoomId == id && x.UserId == userId, ct))
            throw ApiException.Missing();
        return room;
    }

    public async Task<object> ListAsync(Guid userId, CancellationToken ct)
    {
        var rooms = await db.Rooms.AsNoTracking().Where(r => r.OwnerUserId == userId || db.RoomMembers.Any(m => m.RoomId == r.Id && m.UserId == userId))
            .OrderBy(r => r.Name).Take(256).Select(r => new { r.Id, r.Name, r.Slug, r.OwnerUserId }).ToListAsync(ct);
        var invitations = await (from i in db.RoomInvitations.AsNoTracking()
                                 join r in db.Rooms on i.RoomId equals r.Id
                                 join u in db.Users on i.InviterUserId equals u.Id
                                 where i.UserId == userId
                                 orderby i.CreatedAt descending
                                 select new { i.Id, i.RoomId, roomName = r.Name, inviterName = u.DisplayName, i.CreatedAt }).Take(128).ToListAsync(ct);
        return new { rooms, invitations };
    }

    public async Task<object> OpenAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var room = await RequireMemberAsync(id, userId, ct);
        var members = await db.Users.AsNoTracking().Where(u => u.Id == room.OwnerUserId || db.RoomMembers.Any(m => m.RoomId == id && m.UserId == u.Id))
            .OrderBy(u => u.Handle).Select(u => new { userId = u.Id, u.Handle, u.DisplayName, isOwner = u.Id == room.OwnerUserId }).ToListAsync(ct);
        var sessionIds = await db.Sessions.AsNoTracking().Where(s => !s.Ended && s.RoomId == id &&
            (s.OwnerUserId == userId || db.SessionMembers.Any(m => m.SessionId == s.Id && m.UserId == userId)))
            .Select(s => s.Id).ToListAsync(ct);
        return new { room = new { room.Id, room.Name, room.Slug, room.OwnerUserId }, members, sessionIds };
    }

    public async Task<object> CreateAsync(Guid userId, CreateRoom body, CancellationToken ct)
    {
        Limits.Id(body.Id, "Mission ID");
        var name = Limits.Text(body.Name, "Mission name", 128);
        var slug = body.Slug;
        if (string.IsNullOrEmpty(slug) || slug.Length is < 3 or > 64 || slug[0] == '-' || slug[^1] == '-'
            || slug.Any(c => c is not (>= 'a' and <= 'z') && !char.IsAsciiDigit(c) && c != '-'))
            throw ApiException.Invalid("Mission slug must use 3–64 lowercase letters, digits, or internal hyphens.");
        var existing = await db.Rooms.SingleOrDefaultAsync(x => x.Id == body.Id, ct);
        if (existing is not null)
        {
            if (existing.OwnerUserId != userId || existing.Name != name || existing.Slug != slug) throw ApiException.Conflict("Mission ID was reused.");
            return new { existing.Id, existing.Name, existing.Slug, existing.OwnerUserId };
        }
        if (await db.Rooms.CountAsync(x => x.OwnerUserId == userId, ct) >= Limits.MaxRoomsPerUser)
            throw ApiException.Conflict("Too many Missions.");
        if (await db.Rooms.AnyAsync(x => x.Slug == slug, ct)) throw ApiException.Conflict("This Mission slug is already in use.");
        var room = new Room { Id = body.Id, Name = name, Slug = slug, OwnerUserId = userId };
        db.Rooms.Add(room); await db.SaveChangesAsync(ct); relay.Notify(userId, "rooms");
        return new { room.Id, room.Name, room.Slug, room.OwnerUserId };
    }

    public async Task<object> RenameAsync(Guid id, Guid userId, string name, CancellationToken ct)
    {
        var room = await RequireMemberAsync(id, userId, ct);
        if (room.OwnerUserId != userId) throw ApiException.Forbidden();
        room.Name = Limits.Text(name, "Mission name", 128); await db.SaveChangesAsync(ct); await NotifyAsync(room, ct);
        return new { room.Id, room.Name, room.Slug, room.OwnerUserId };
    }

    public async Task DeleteAsync(Guid id, Guid userId, CancellationToken ct)
    {
        var room = await db.Rooms.SingleOrDefaultAsync(x => x.Id == id, ct);
        if (room is null) return;
        if (room.OwnerUserId != userId) throw ApiException.Forbidden();
        var users = await MemberIdsAsync(room, ct);
        var viewers = await AffectedSessionUsers(id, null, ct);
        db.Rooms.Remove(room); await db.SaveChangesAsync(ct);
        foreach (var member in users) relay.Notify(member, "rooms");
        foreach (var viewer in viewers.Concat(users).Distinct()) relay.Notify(viewer, "sessions");
    }

    public async Task InviteAsync(Guid id, Guid userId, Invite body, CancellationToken ct)
    {
        var room = await RequireMemberAsync(id, userId, ct);
        if (room.OwnerUserId != userId) throw ApiException.Forbidden();
        if (!await friends.AreFriendsAsync(userId, body.UserId, ct)) throw ApiException.Forbidden("Invite an accepted friend.");
        if (await db.RoomMembers.AnyAsync(x => x.RoomId == id && x.UserId == body.UserId, ct)) return;
        if (await db.RoomMembers.CountAsync(x => x.RoomId == id, ct) + await db.RoomInvitations.CountAsync(x => x.RoomId == id, ct) >= Limits.MaxRoomMembers)
            throw ApiException.Conflict("This Mission has reached its member limit.");
        if (!await db.RoomInvitations.AnyAsync(x => x.RoomId == id && x.UserId == body.UserId, ct))
        {
            db.RoomInvitations.Add(new RoomInvitation { Id = Limits.Id(body.Id, "Invitation ID"), RoomId = id, UserId = body.UserId, InviterUserId = userId, CreatedAt = clock.GetUtcNow() });
            await db.SaveChangesAsync(ct);
        }
        relay.Notify(body.UserId, "rooms");
    }

    public async Task ResolveInvitationAsync(Guid id, Guid userId, bool accept, CancellationToken ct)
    {
        var invitation = await db.RoomInvitations.SingleOrDefaultAsync(x => x.Id == id && x.UserId == userId, ct) ?? throw ApiException.Missing();
        var room = await db.Rooms.SingleAsync(x => x.Id == invitation.RoomId, ct);
        if (accept)
        {
            if (!await friends.AreFriendsAsync(room.OwnerUserId, userId, ct)) throw ApiException.Forbidden("This invitation is no longer available.");
            if (!await db.RoomMembers.AnyAsync(x => x.RoomId == room.Id && x.UserId == userId, ct))
                db.RoomMembers.Add(new RoomMember { RoomId = room.Id, UserId = userId });
        }
        db.RoomInvitations.Remove(invitation); await db.SaveChangesAsync(ct); await NotifyAsync(room, ct); relay.Notify(userId, "rooms");
    }

    public async Task RemoveMemberAsync(Guid id, Guid actor, Guid userId, CancellationToken ct)
    {
        var room = await RequireMemberAsync(id, actor, ct);
        if (userId == room.OwnerUserId) throw ApiException.Invalid("The owner must delete the Mission instead of leaving.");
        if (actor != userId && room.OwnerUserId != actor) throw ApiException.Forbidden();
        var viewers = await AffectedSessionUsers(id, userId, ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        await db.RoomMembers.Where(x => x.RoomId == id && x.UserId == userId).ExecuteDeleteAsync(ct);
        await db.RoomInvitations.Where(x => x.RoomId == id && x.UserId == userId).ExecuteDeleteAsync(ct);
        await db.Sessions.Where(x => x.RoomId == id && x.OwnerUserId == userId).ExecuteUpdateAsync(set => set.SetProperty(x => x.RoomId, (Guid?)null), ct);
        await transaction.CommitAsync(ct);
        await NotifyAsync(room, ct); relay.Notify(userId, "rooms");
        foreach (var viewer in viewers.Append(userId).Distinct()) relay.Notify(viewer, "sessions");
    }

    private async Task<List<Guid>> AffectedSessionUsers(Guid roomId, Guid? owner, CancellationToken ct)
    {
        var affected = db.Sessions.Where(s => s.RoomId == roomId && (owner == null || s.OwnerUserId == owner));
        return await affected.Select(s => s.OwnerUserId)
            .Union(db.SessionMembers.Where(m => affected.Any(s => s.Id == m.SessionId)).Select(m => m.UserId)).ToListAsync(ct);
    }

    private async Task<List<Guid>> MemberIdsAsync(Room room, CancellationToken ct)
    {
        var ids = await db.RoomMembers.Where(x => x.RoomId == room.Id).Select(x => x.UserId).ToListAsync(ct);
        ids.Add(room.OwnerUserId); return ids;
    }
    private async Task NotifyAsync(Room room, CancellationToken ct)
    {
        foreach (var id in await MemberIdsAsync(room, ct)) relay.Notify(id, "rooms");
    }
    public sealed record CreateRoom(Guid Id, string Name, string Slug);
    public sealed record RenameRoom(string Name);
    public sealed record Invite(Guid Id, Guid UserId);
}

internal static class RoomEndpoints
{
    public static void MapRooms(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/rooms").RequireAuthorization();
        group.MapGet("/", async (HttpContext ctx, CurrentUser users, DeviceService devices, RoomService rooms, CancellationToken ct) =>
        { var id = await Actor(ctx, users, devices, ct); return Results.Ok(await rooms.ListAsync(id, ct)); });
        group.MapPost("/", async (RoomService.CreateRoom body, HttpContext ctx, CurrentUser users, DeviceService devices, RoomService rooms, CancellationToken ct) =>
        { var id = await Actor(ctx, users, devices, ct); return Results.Ok(await rooms.CreateAsync(id, body, ct)); });
        group.MapGet("/{id:guid}", async (Guid id, HttpContext ctx, CurrentUser users, DeviceService devices, RoomService rooms, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await rooms.OpenAsync(id, actor, ct)); });
        group.MapPatch("/{id:guid}", async (Guid id, RoomService.RenameRoom body, HttpContext ctx, CurrentUser users, DeviceService devices, RoomService rooms, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); return Results.Ok(await rooms.RenameAsync(id, actor, body.Name, ct)); });
        group.MapDelete("/{id:guid}", async (Guid id, HttpContext ctx, CurrentUser users, DeviceService devices, RoomService rooms, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); await rooms.DeleteAsync(id, actor, ct); return Results.NoContent(); });
        group.MapPost("/{id:guid}/invitations", async (Guid id, RoomService.Invite body, HttpContext ctx, CurrentUser users, DeviceService devices, RoomService rooms, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); await rooms.InviteAsync(id, actor, body, ct); return Results.NoContent(); });
        foreach (var action in new[] { "accept", "reject" })
            group.MapPost($"/invitations/{{id:guid}}/{action}", async (Guid id, HttpContext ctx, CurrentUser users, DeviceService devices, RoomService rooms, CancellationToken ct) =>
            { var actor = await Actor(ctx, users, devices, ct); await rooms.ResolveInvitationAsync(id, actor, action == "accept", ct); return Results.NoContent(); });
        group.MapDelete("/{id:guid}/members/{userId:guid}", async (Guid id, Guid userId, HttpContext ctx, CurrentUser users, DeviceService devices, RoomService rooms, CancellationToken ct) =>
        { var actor = await Actor(ctx, users, devices, ct); await rooms.RemoveMemberAsync(id, actor, userId, ct); return Results.NoContent(); });
    }
    private static async Task<Guid> Actor(HttpContext ctx, CurrentUser users, DeviceService devices, CancellationToken ct)
    { var user = await users.GetAsync(ctx, ct); await devices.RequireProofAsync(ctx, user.Id, ct); return user.Id; }
}
