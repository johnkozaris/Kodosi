using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Plans;
using Kodosi.Security;
using Kodosi.Sessions;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Friends;

public sealed class FriendService(KodosiDbContext db, ConnectionDirectory connections, TimeProvider clock, PlanCatalog plans, NameChecks names)
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
            throw ApiException.OverLimit("friends", most, $"You have {Limits.Count(count, "friend", "friends")}, the most that you can have. Remove a friend first.");
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
    private const int FriendListPage = 32;
    private const int KeptFriendListRevisions = 1024;
    private const int FriendPairLocks = 1;

    public async Task<object> SignedListAsync(Guid user, long? after, CancellationToken ct)
    {
        var stored = db.FriendLists.AsNoTracking().Where(x => x.UserId == user);
        var latest = await stored.MaxAsync(x => (long?)x.Revision, ct) ?? throw ApiException.Missing();
        var page = after is { } from
            ? await stored.Where(x => x.Revision > from).OrderBy(x => x.Revision).Take(FriendListPage).ToListAsync(ct)
            : (await stored.OrderByDescending(x => x.Revision).Take(FriendListPage).ToListAsync(ct)).AsEnumerable().Reverse().ToList();
        return new
        {
            revision = latest,
            lists = page.Select(x => new { body = Convert.ToBase64String(x.Body), signature = Convert.ToBase64String(x.Signature) }),
        };
    }

    public async Task ReplaceSignedListAsync(Guid user, SignedFriendList request, CancellationToken ct)
    {
        var body = Limits.Base64(request.Body, "Friend list", MaxFriendListBodyLength);
        var signature = Limits.Base64(request.Signature, "Friend list signature", IdentityWireFormat.MlDsa65SignatureLength);
        if (body.Length == 0 || signature.Length != IdentityWireFormat.MlDsa65SignatureLength || request.Revision <= request.ExpectedRevision)
            throw ApiException.Invalid("The friend list is not valid.");
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        await db.LockUserAsync(user, ct);
        var latest = await db.FriendLists.Where(x => x.UserId == user).MaxAsync(x => (long?)x.Revision, ct) ?? 0;
        if (latest != request.ExpectedRevision) throw ApiException.Conflict("The friend list changed on another device.");
        if (latest != 0 && request.Revision != latest + 1) throw ApiException.Invalid("The friend list is not valid.");
        db.FriendLists.Add(new FriendList { UserId = user, Revision = request.Revision, Body = body, Signature = signature });
        await db.SaveChangesAsync(ct);
        var oldest = await db.FriendLists.Where(x => x.UserId == user).OrderByDescending(x => x.Revision)
            .Skip(KeptFriendListRevisions).Select(x => (long?)x.Revision).FirstOrDefaultAsync(ct);
        if (oldest is { } dropped) await db.FriendLists.Where(x => x.UserId == user && x.Revision <= dropped).ExecuteDeleteAsync(ct);
        await transaction.CommitAsync(ct);
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
        var other = await db.Users.SingleOrDefaultAsync(x => x.Handle == handle, ct);
        var reaches = action is "send" or "block";
        if (reaches && (other is null || !await KnowsAsync(userId, other.Id, ct))) await names.TakeAsync(userId, ct);
        if (other is null)
        {
            if (reaches) throw ApiException.Missing("No Kodosi account has this user name.");
            if (action == "accept") throw ApiException.Missing();
            return;
        }
        if (other.Id == userId) throw ApiException.Invalid("Choose another person.");
        var (first, second) = Pair(userId, other.Id);
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        await db.Database.ExecuteSqlInterpolatedAsync($"SELECT pg_advisory_xact_lock({FriendPairLocks}, {PairLock(first, second)})", ct);
        var link = await db.Friendships.FromSqlInterpolated(
            $"SELECT * FROM friendships WHERE \"FirstUserId\" = {first} AND \"SecondUserId\" = {second} FOR UPDATE").SingleOrDefaultAsync(ct);
        List<Session> shared = [];
        switch (action)
        {
            case "send":
                if (link is not null) return;
                await RequireUnblockedAsync(userId, other.Id, ct);
                var mine = await plans.LimitsAsync(db, userId, ct);
                var sent = await OpenRequests(userId).CountAsync(x => x.RequestedBy == userId, ct);
                if (sent >= mine.SentFriendRequests)
                    throw ApiException.OverLimit("friendRequests", mine.SentFriendRequests, $"You have {Limits.Count(sent, "open friend request", "open friend requests")}. Wait for an answer, or cancel a request.");
                await RequireFriendSpaceAsync(userId, other.Id, ct);
                if ((await plans.LimitsAsync(db, other.Id, ct)).ReceivedFriendRequests is { } received
                    && await OpenRequests(other.Id).CountAsync(x => x.RequestedBy != other.Id, ct) >= received)
                    throw ApiException.Forbidden("This person has too many open friend requests. Try again later.");
                db.Friendships.Add(new Friendship { FirstUserId = first, SecondUserId = second, RequestedBy = userId, CreatedAt = clock.GetUtcNow() });
                break;
            case "accept":
                if (link is null || (link.RequestedBy == userId && !link.Accepted)) throw ApiException.Missing();
                if (link.Accepted) return;
                await RequireUnblockedAsync(userId, other.Id, ct);
                await RequireFriendSpaceAsync(userId, other.Id, ct);
                link.Accepted = true;
                shared = await TerminalAccess.InProjectsBetween(db, userId, other.Id).ToListAsync(ct);
                foreach (var session in shared) session.AuthorizationRevision = checked(session.AuthorizationRevision + 1);
                break;
            case "reject":
            case "cancel":
                if (link is null) return;
                if (link.Accepted || (action == "cancel") != (link.RequestedBy == userId)) throw ApiException.Forbidden();
                db.Friendships.Remove(link);
                break;
            case "remove":
            case "block":
                if (link is null && action == "remove") return;
                if (link is not null)
                {
                    db.Friendships.Remove(link);
                    shared = await EndSharingAsync(userId, other.Id, ct);
                }
                if (action == "block" && !await db.Blocks.AnyAsync(x => x.BlockerId == userId && x.BlockedId == other.Id, ct))
                    db.Blocks.Add(new Block { BlockerId = userId, BlockedId = other.Id, CreatedAt = clock.GetUtcNow() });
                break;
            case "unblock":
                var blocks = await db.Blocks.Where(x => x.BlockerId == userId && x.BlockedId == other.Id).ToListAsync(ct);
                if (blocks.Count == 0) return;
                db.Blocks.RemoveRange(blocks);
                break;
            default: throw ApiException.Invalid("Unknown friend operation.");
        }
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        connections.Notify([userId, other.Id], "friends");
        if (shared.Count == 0) return;
        await TerminalAccess.RevokeAsync(db, connections, shared, ct);
        connections.Notify([userId, other.Id], "sessions");
    }

    public async Task<object> BlockedAsync(Guid user, CancellationToken ct) =>
        await db.Blocks.AsNoTracking().Where(x => x.BlockerId == user)
            .Join(db.Users, block => block.BlockedId, person => person.Id, (_, person) => person)
            .OrderBy(x => x.Handle).Select(x => new { userId = x.Id, x.Handle, x.DisplayName }).ToListAsync(ct);

    private async Task<List<Session>> EndSharingAsync(Guid userId, Guid otherId, CancellationToken ct)
    {
        var grants = await db.SessionMembers.Where(m =>
            (m.UserId == otherId && db.Sessions.Any(s => s.Id == m.SessionId && s.OwnerUserId == userId)) ||
            (m.UserId == userId && db.Sessions.Any(s => s.Id == m.SessionId && s.OwnerUserId == otherId))).ToListAsync(ct);
        db.SessionMembers.RemoveRange(grants);
        var sessionIds = grants.Select(x => x.SessionId).Distinct().ToArray();
        var shared = await db.Sessions.Where(x => sessionIds.Contains(x.Id) && !x.Ended)
            .Union(TerminalAccess.InProjectsBetween(db, userId, otherId)).ToListAsync(ct);
        foreach (var session in shared) session.AuthorizationRevision = checked(session.AuthorizationRevision + 1);
        return shared;
    }

    /// One lock for each pair of people, so that a block, a request and an answer between
    /// them take turns even before they have a friendship row.
    private static int PairLock(Guid first, Guid second)
    {
        Span<byte> pair = stackalloc byte[32];
        first.TryWriteBytes(pair);
        second.TryWriteBytes(pair[16..]);
        return BitConverter.ToInt32(System.Security.Cryptography.SHA256.HashData(pair));
    }

    private async Task<bool> KnowsAsync(Guid userId, Guid otherId, CancellationToken ct)
    {
        var (first, second) = Pair(userId, otherId);
        return await db.Friendships.AnyAsync(x => x.FirstUserId == first && x.SecondUserId == second, ct);
    }

    private async Task RequireUnblockedAsync(Guid userId, Guid otherId, CancellationToken ct)
    {
        if (await db.Blocks.AnyAsync(x => x.BlockerId == userId && x.BlockedId == otherId, ct))
            throw ApiException.Forbidden("Unblock this person first.");
        if (await db.Blocks.AnyAsync(x => x.BlockerId == otherId && x.BlockedId == userId, ct))
            throw ApiException.Forbidden("This person does not take friend requests from you.");
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
        group.MapGet("/blocked", async (DeviceCaller caller, FriendService friends, CancellationToken ct) =>
            Results.Ok(await friends.BlockedAsync(caller.User.Id, ct)));
        group.MapPut("/blocked/{username}", async (string username, DeviceCaller caller, FriendService friends, CancellationToken ct) =>
        {
            await friends.MutateAsync(caller.User.Id, username, "block", ct);
            return Results.NoContent();
        });
        group.MapDelete("/blocked/{username}", async (string username, DeviceCaller caller, FriendService friends, CancellationToken ct) =>
        {
            await friends.MutateAsync(caller.User.Id, username, "unblock", ct);
            return Results.NoContent();
        });
        group.MapReports();
        var list = app.MapGroup("/api/me/friend-list").RequireAuthorization();
        list.MapGet("/", async (long? after, DeviceCaller caller, FriendService friends, CancellationToken ct) =>
            Results.Ok(await friends.SignedListAsync(caller.User.Id, after, ct)));
        list.MapPut("/", async (FriendService.SignedFriendList body, DeviceCaller caller, FriendService friends, CancellationToken ct) =>
        {
            await friends.ReplaceSignedListAsync(caller.User.Id, body, ct);
            return Results.NoContent();
        });
    }

    private sealed record FriendRequest(string Username);
}
