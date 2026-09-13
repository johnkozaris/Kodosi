using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Realtime;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Friends;

public sealed class FriendService(KodosiDbContext db, RelayDirectory relay, TimeProvider clock)
{
    public static (Guid First, Guid Second) Pair(Guid a, Guid b) => a.CompareTo(b) < 0 ? (a, b) : (b, a);

    public async Task<bool> AreFriendsAsync(Guid a, Guid b, CancellationToken ct)
    {
        var (first, second) = Pair(a, b);
        return await db.Friendships.AnyAsync(x => x.FirstUserId == first && x.SecondUserId == second && x.Accepted, ct);
    }

    public async Task<object> ListAsync(Guid user, CancellationToken ct)
    {
        var links = await db.Friendships.AsNoTracking().Where(x => (x.FirstUserId == user || x.SecondUserId == user) && x.Accepted).ToListAsync(ct);
        var ids = links.Select(x => x.FirstUserId == user ? x.SecondUserId : x.FirstUserId).ToArray();
        return await db.Users.AsNoTracking().Where(x => ids.Contains(x.Id))
            .OrderBy(x => x.Handle).Select(x => new { userId = x.Id, x.Handle, x.DisplayName, x.AvatarUrl }).ToListAsync(ct);
    }

    public async Task<object> RequestsAsync(Guid user, CancellationToken ct)
    {
        var requests = await db.Friendships.AsNoTracking().Where(x => (x.FirstUserId == user || x.SecondUserId == user) && !x.Accepted).ToListAsync(ct);
        var ids = requests.Select(x => x.FirstUserId == user ? x.SecondUserId : x.FirstUserId).ToArray();
        var people = await db.Users.AsNoTracking().Where(x => ids.Contains(x.Id)).ToDictionaryAsync(x => x.Id, ct);
        object Project(Friendship request)
        {
            var person = people[request.FirstUserId == user ? request.SecondUserId : request.FirstUserId];
            return new { userId = person.Id, person.Handle, person.DisplayName, person.AvatarUrl, request.CreatedAt };
        }
        return new { incoming = requests.Where(x => x.RequestedBy != user).Select(Project), outgoing = requests.Where(x => x.RequestedBy == user).Select(Project) };
    }

    public async Task MutateAsync(Guid userId, string username, string action, CancellationToken ct)
    {
        var handle = Limits.Text(username, "Username", 64).ToLowerInvariant();
        var other = await db.Users.SingleOrDefaultAsync(x => x.Handle == handle, ct) ?? throw ApiException.Missing();
        if (other.Id == userId) throw ApiException.Invalid("Choose another person.");
        var (first, second) = Pair(userId, other.Id);
        var link = await db.Friendships.SingleOrDefaultAsync(x => x.FirstUserId == first && x.SecondUserId == second, ct);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var affected = new List<Session>();
        switch (action)
        {
            case "send":
                if (link is not null) return;
                if (await db.Friendships.CountAsync(x => x.RequestedBy == userId && !x.Accepted, ct) >= 64)
                    throw new ApiException(429, "Too many outstanding friend requests.");
                db.Friendships.Add(new Friendship { FirstUserId = first, SecondUserId = second, RequestedBy = userId, CreatedAt = clock.GetUtcNow() });
                break;
            case "accept":
                if (link is null || (link.RequestedBy == userId && !link.Accepted)) throw ApiException.Missing();
                if (link.Accepted) return;
                link.Accepted = true;
                break;
            case "reject":
            case "cancel":
                if (link is null) return;
                if (link.Accepted || (action == "cancel") != (link.RequestedBy == userId)) throw ApiException.Forbidden();
                db.Friendships.Remove(link);
                break;
            case "remove":
                if (link is null) return;
                db.Friendships.Remove(link);
                var grants = await db.SessionMembers.Where(m =>
                    (m.UserId == other.Id && db.Sessions.Any(s => s.Id == m.SessionId && s.OwnerUserId == userId)) ||
                    (m.UserId == userId && db.Sessions.Any(s => s.Id == m.SessionId && s.OwnerUserId == other.Id))).ToListAsync(ct);
                db.SessionMembers.RemoveRange(grants);
                var sessionIds = grants.Select(x => x.SessionId).Distinct().ToArray();
                affected = await db.Sessions.Where(x => sessionIds.Contains(x.Id) && !x.Ended).ToListAsync(ct);
                foreach (var session in affected)
                {
                    session.Ready = false; session.AuthorizationRevision = checked(session.AuthorizationRevision + 1);
                    session.KeyGeneration = checked(session.KeyGeneration + 1);
                    relay.Invalidate(session, notifyHost: false);
                    await db.SessionKeys.Where(x => x.SessionId == session.Id).ExecuteDeleteAsync(ct);
                }
                break;
            default: throw ApiException.Invalid("Unknown friend operation.");
        }
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        foreach (var session in affected) relay.Invalidate(session);
        relay.Notify(userId, "friends"); relay.Notify(other.Id, "friends");
        relay.Notify(userId, "sessions"); relay.Notify(other.Id, "sessions");
    }
}

internal static class FriendEndpoints
{
    public static void MapFriends(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/friends").RequireAuthorization();
        group.MapGet("/", async (HttpContext ctx, CurrentUser users, DeviceService devices, FriendService friends, CancellationToken ct) =>
        {
            var user = await users.GetAsync(ctx, ct); await devices.RequireProofAsync(ctx, user.Id, ct);
            return Results.Ok(await friends.ListAsync(user.Id, ct));
        });
        group.MapGet("/requests", async (HttpContext ctx, CurrentUser users, DeviceService devices, FriendService friends, CancellationToken ct) =>
        {
            var user = await users.GetAsync(ctx, ct); await devices.RequireProofAsync(ctx, user.Id, ct);
            return Results.Ok(await friends.RequestsAsync(user.Id, ct));
        });
        group.MapPost("/requests", async (FriendRequest body, HttpContext ctx, CurrentUser users, DeviceService devices, FriendService friends, CancellationToken ct) =>
        {
            var user = await users.GetAsync(ctx, ct); await devices.RequireProofAsync(ctx, user.Id, ct);
            await friends.MutateAsync(user.Id, body.Username, "send", ct); return Results.NoContent();
        });
        foreach (var action in new[] { "accept", "reject", "cancel" })
            group.MapPost($"/requests/{{username}}/{action}", async (string username, HttpContext ctx, CurrentUser users, DeviceService devices, FriendService friends, CancellationToken ct) =>
            {
                var user = await users.GetAsync(ctx, ct); await devices.RequireProofAsync(ctx, user.Id, ct);
                await friends.MutateAsync(user.Id, username, action, ct); return Results.NoContent();
            });
        group.MapDelete("/{username}", async (string username, HttpContext ctx, CurrentUser users, DeviceService devices, FriendService friends, CancellationToken ct) =>
        {
            var user = await users.GetAsync(ctx, ct); await devices.RequireProofAsync(ctx, user.Id, ct);
            await friends.MutateAsync(user.Id, username, "remove", ct); return Results.NoContent();
        });
    }
    private sealed record FriendRequest(string Username);
}
