using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class DiscoveryAudienceResolver(
    IFriendshipRepository friendships,
    IRoomMemberRepository roomMembers,
    ISessionViewerDismissalRepository dismissals)
{
    private readonly IFriendshipRepository _friendships = friendships;
    private readonly IRoomMemberRepository _roomMembers = roomMembers;
    private readonly ISessionViewerDismissalRepository _dismissals = dismissals;

    public async Task<DiscoveryAudience> ResolveSessionChangeAsync(
        SessionDiscoveryTarget? before,
        SessionDiscoveryTarget? after,
        CancellationToken ct = default)
    {
        var states = new[] { before, after }
            .Where(state => state is not null)
            .Select(state => state!)
            .Distinct()
            .ToList();

        if (states.Count == 0)
        {
            return DiscoveryAudience.None;
        }

        var userIds = new HashSet<UserId>();
        foreach (var state in states)
        {
            await AddSessionAudienceAsync(userIds, state, ct);
        }

        var sessionIds = states.Select(state => state.SessionId).Distinct().ToList();
        var dismissedUserIds = await _dismissals.GetDismissedViewerIdsAsync(
            sessionIds,
            userIds,
            ct);
        userIds.ExceptWith(dismissedUserIds);

        return DiscoveryAudience.ForUsers(userIds);
    }

    public DiscoveryAudience ResolveExactUsers(params UserId[] userIds)
        => DiscoveryAudience.ForUsers(userIds);

    public async Task<DiscoveryAudience> ResolveRoomMembersAsync(
        RoomId roomId,
        UserId includeUserId,
        CancellationToken ct = default)
    {


        var members = await _roomMembers.GetMemberUserIdsAsync(roomId, ct);
        var audience = new HashSet<UserId>(members) { includeUserId };
        return DiscoveryAudience.ForUsers(audience);
    }

    private async Task AddSessionAudienceAsync(
        HashSet<UserId> userIds,
        SessionDiscoveryTarget state,
        CancellationToken ct)
    {
        switch (state.Scope)
        {
            case SessionScope.JustMe:
                return;
            case SessionScope.MyDevices:
                userIds.Add(state.OwnerUserId);
                return;
            case SessionScope.Friends:
                userIds.Add(state.OwnerUserId);
                userIds.UnionWith(await _friendships.GetFriendIdsAsync(state.OwnerUserId, ct));
                return;
            case SessionScope.Room:
                if (state.RoomId is null)
                {
                    return;
                }

                userIds.UnionWith(await _roomMembers.GetMemberUserIdsAsync(state.RoomId.Value, ct));
                return;
            default:
                throw new InvalidOperationException($"Unsupported session scope '{state.Scope}'.");
        }
    }
}

public sealed class DiscoveryAudience
{
    private static readonly IReadOnlySet<UserId> EmptyUserSet = new HashSet<UserId>();

    private DiscoveryAudience(IReadOnlySet<UserId> userIds)
    {
        UserIds = userIds;
    }

    public IReadOnlySet<UserId> UserIds { get; }

    public static DiscoveryAudience None { get; } = new(EmptyUserSet);

    public static DiscoveryAudience ForUsers(IEnumerable<UserId> userIds)
    {
        var resolved = userIds.Distinct().ToHashSet();
        return resolved.Count == 0
            ? None
            : new DiscoveryAudience(resolved);
    }
}
