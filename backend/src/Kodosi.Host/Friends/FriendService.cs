using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Plans;
using Kodosi.Security;
using Kodosi.Sessions;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Friends;

public sealed class FriendService(KodosiDbContext db, ConnectionDirectory connections, TimeProvider clock, PlanCatalog plans)
{
    public static (Guid First, Guid Second) Pair(Guid a, Guid b) => a.CompareTo(b) < 0 ? (a, b) : (b, a);

    public async Task<bool> AreFriendsAsync(Guid a, Guid b, CancellationToken ct)
    {
        var (first, second) = Pair(a, b);
        return await db.Friendships.AnyAsync(x => x.FirstUserId == first && x.SecondUserId == second && x.Accepted, ct);
    }

    private async Task RequireFriendSpaceAsync(Guid userId, Guid otherId, CancellationToken ct)
    {
        if ((await plans.LimitsAsync(db, userId, ct)).Friends is { } most && await FriendCountAsync(userId, ct) is var count && count >= most)
            throw ApiException.Forbidden($"You have {Limits.Count(count, "friend", "friends")}, the most that you can have. Remove a friend first.");
        if ((await plans.LimitsAsync(db, otherId, ct)).Friends is { } theirs && await FriendCountAsync(otherId, ct) >= theirs)
            throw ApiException.Forbidden("This person has the most friends that they can have.");
    }

    private Task<int> FriendCountAsync(Guid user, CancellationToken ct)
        => db.Friendships.CountAsync(x => (x.FirstUserId == user || x.SecondUserId == user) && x.Accepted, ct);

    public async Task<object> ListAsync(Guid user, CancellationToken ct)
    {
        var links = await db.Friendships.AsNoTracking().Where(x => (x.FirstUserId == user || x.SecondUserId == user) && x.Accepted).ToListAsync(ct);
        var ids = links.Select(x => x.FirstUserId == user ? x.SecondUserId : x.FirstUserId).ToArray();
        return await db.Users.AsNoTracking().Where(x => ids.Contains(x.Id))
            .OrderBy(x => x.Handle).Select(x => new { userId = x.Id, x.Handle, x.DisplayName, x.IdentityIncarnationId }).ToListAsync(ct);
    }

    private const int MaxFriendListBodyLength = 256 * 1024;

    public async Task<object> SignedListAsync(Guid user, CancellationToken ct)
    {
        var list = await db.FriendLists.AsNoTracking().SingleOrDefaultAsync(x => x.UserId == user, ct) ?? throw ApiException.Missing();
        return new { list.Revision, body = Convert.ToBase64String(list.Body), signature = Convert.ToBase64String(list.Signature) };
    }

    public async Task ReplaceSignedListAsync(Guid user, SignedFriendList request, CancellationToken ct)
    {
        var body = Limits.Base64(request.Body, "Friend list", MaxFriendListBodyLength);
        var signature = Limits.Base64(request.Signature, "Friend list signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (body.Length == 0 || signature.Length != IdentityWireFormat.MlDsa65SignatureLength || request.Revision <= request.ExpectedRevision)
            throw ApiException.Invalid("The friend list is not valid.");
        var list = await db.FriendLists.SingleOrDefaultAsync(x => x.UserId == user, ct);
        if ((list?.Revision ?? 0) != request.ExpectedRevision) throw ApiException.Conflict("The friend list changed on another device.");
        if (list is null) db.FriendLists.Add(list = new FriendList { UserId = user });
        list.Revision = request.Revision; list.Body = body; list.Signature = signature;
        await db.SaveChangesAsync(ct);
        connections.Notify([user], "friends");
    }

    public sealed record SignedFriendList(long ExpectedRevision, long Revision, string Body, string Signature);

    public async Task<object> RequestsAsync(Guid user, CancellationToken ct)
    {
        var requests = await db.Friendships.AsNoTracking().Where(x => (x.FirstUserId == user || x.SecondUserId == user) && !x.Accepted).ToListAsync(ct);
        var ids = requests.Select(x => x.FirstUserId == user ? x.SecondUserId : x.FirstUserId).ToArray();
        var people = await db.Users.AsNoTracking().Where(x => ids.Contains(x.Id)).ToDictionaryAsync(x => x.Id, ct);
        object Project(Friendship request)
        {
            var person = people[request.FirstUserId == user ? request.SecondUserId : request.FirstUserId];
            return new { userId = person.Id, person.Handle, person.DisplayName, request.CreatedAt };
        }
        return new { incoming = requests.Where(x => x.RequestedBy != user).Select(Project), outgoing = requests.Where(x => x.RequestedBy == user).Select(Project) };
    }

    public async Task MutateAsync(Guid userId, string username, string action, CancellationToken ct)
    {
        var handle = Limits.Text(username, "Username", 64).ToLowerInvariant();
        var other = await db.Users.SingleOrDefaultAsync(x => x.Handle == handle, ct) ?? throw ApiException.Missing();
        if (other.Id == userId) throw ApiException.Invalid("Choose another person.");
        var (first, second) = Pair(userId, other.Id);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var link = await db.Friendships.FromSqlInterpolated(
            $"SELECT * FROM friendships WHERE \"FirstUserId\" = {first} AND \"SecondUserId\" = {second} FOR UPDATE").SingleOrDefaultAsync(ct);
        List<Session> shared = [];
        switch (action)
        {
            case "send":
                if (link is not null) return;
                var mine = await plans.LimitsAsync(db, userId, ct);
                var sent = await OpenRequests(userId).CountAsync(x => x.RequestedBy == userId, ct);
                if (sent >= mine.SentFriendRequests)
                    throw ApiException.Forbidden($"You have {Limits.Count(sent, "open friend request", "open friend requests")}. Wait for an answer, or cancel a request.");
                await RequireFriendSpaceAsync(userId, other.Id, ct);
                if ((await plans.LimitsAsync(db, other.Id, ct)).ReceivedFriendRequests is { } received
                    && await OpenRequests(other.Id).CountAsync(x => x.RequestedBy != other.Id, ct) >= received)
                    throw ApiException.Forbidden("This person has too many open friend requests. Try again later.");
                db.Friendships.Add(new Friendship { FirstUserId = first, SecondUserId = second, RequestedBy = userId, CreatedAt = clock.GetUtcNow() });
                break;
            case "accept":
                if (link is null || (link.RequestedBy == userId && !link.Accepted)) throw ApiException.Missing();
                if (link.Accepted) return;
                await RequireFriendSpaceAsync(userId, other.Id, ct);
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
                shared = await db.Sessions.Where(x => sessionIds.Contains(x.Id) && !x.Ended).ToListAsync(ct);
                foreach (var session in shared) session.AuthorizationRevision = checked(session.AuthorizationRevision + 1);
                break;
            default: throw ApiException.Invalid("Unknown friend operation.");
        }
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        connections.Notify([userId, other.Id], "friends");
        if (shared.Count == 0) return;
        await TerminalAccess.RevokeAsync(db, connections, shared, ct);
        connections.Notify([userId, other.Id], "sessions");
    }

    private IQueryable<Friendship> OpenRequests(Guid user) =>
        db.Friendships.Where(x => (x.FirstUserId == user || x.SecondUserId == user) && !x.Accepted);
}

internal static class FriendEndpoints
{
    public static void MapFriends(this IEndpointRouteBuilder app)
    {
        var group = app.MapGroup("/api/friends").RequireAuthorization();
        group.MapGet("/", async (DeviceCaller caller, FriendService friends, CancellationToken ct) =>
            Results.Ok(await friends.ListAsync(caller.User.Id, ct)));
        group.MapGet("/requests", async (DeviceCaller caller, FriendService friends, CancellationToken ct) =>
            Results.Ok(await friends.RequestsAsync(caller.User.Id, ct)));
        group.MapPost("/requests", async (FriendRequest body, DeviceCaller caller, FriendService friends, CancellationToken ct) =>
        {
            await friends.MutateAsync(caller.User.Id, body.Username, "send", ct);
            return Results.NoContent();
        });
        foreach (var action in new[] { "accept", "reject", "cancel" })
            group.MapPost($"/requests/{{username}}/{action}", async (string username, DeviceCaller caller, FriendService friends, CancellationToken ct) =>
            {
                await friends.MutateAsync(caller.User.Id, username, action, ct);
                return Results.NoContent();
            });
        group.MapDelete("/{username}", async (string username, DeviceCaller caller, FriendService friends, CancellationToken ct) =>
        {
            await friends.MutateAsync(caller.User.Id, username, "remove", ct);
            return Results.NoContent();
        });
        var list = app.MapGroup("/api/me/friend-list").RequireAuthorization();
        list.MapGet("/", async (DeviceCaller caller, FriendService friends, CancellationToken ct) =>
            Results.Ok(await friends.SignedListAsync(caller.User.Id, ct)));
        list.MapPut("/", async (FriendService.SignedFriendList body, DeviceCaller caller, FriendService friends, CancellationToken ct) =>
        {
            await friends.ReplaceSignedListAsync(caller.User.Id, body, ct);
            return Results.NoContent();
        });
    }

    private sealed record FriendRequest(string Username);
}
