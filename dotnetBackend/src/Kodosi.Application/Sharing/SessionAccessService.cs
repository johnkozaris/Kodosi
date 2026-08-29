using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionAccessService(
    IFriendshipRepository friendships,
    IRoomMemberRepository roomMembers,
    IAccessOverrideRepository overrides,
    ISessionViewerDismissalRepository dismissals,
    TimeProvider? timeProvider = null)
{
    private readonly IFriendshipRepository _friendships = friendships;
    private readonly IRoomMemberRepository _roomMembers = roomMembers;
    private readonly IAccessOverrideRepository _overrides = overrides;
    private readonly ISessionViewerDismissalRepository _dismissals = dismissals;
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    public async Task<AccessLevel> ResolveAccessAsync(
        Session session,
        UserId actorUserId,
        CancellationToken ct = default)
    {
        var isOwner = session.IsOwner(actorUserId);
        if (!isOwner && await _dismissals.ExistsAsync(session.Id, actorUserId, ct))
        {
            throw new PolicyViolationException("Session access was dismissed by this viewer.");
        }

        var isFriend = session.Scope == SessionScope.Friends
            ? await _friendships.AreFriendsAsync(actorUserId, session.OwnerUserId, ct)
            : false;

        var isRoomMember = session.Scope == SessionScope.Room && session.RoomId.HasValue
            ? await _roomMembers.IsMemberAsync(session.RoomId.Value, actorUserId, ct)
            : false;

        var explicitOverride = await _overrides.GetActiveAsync(session.Id, actorUserId, ct);
        AccessLevel? overrideLevel = explicitOverride?.IsActiveAt(_timeProvider.GetUtcNow()) == true
            ? explicitOverride.AccessLevel
            : null;

        return AccessResolver.ResolveAccess(
            isOwner,
            session.Scope,
            session.DefaultAccess,
            isFriend,
            isRoomMember,
            overrideLevel);
    }

    public async Task<IReadOnlySet<UserId>> GetDeniedUserIdsAsync(
        Session session,
        IReadOnlyCollection<UserId> actorUserIds,
        CancellationToken ct = default)
    {
        if (actorUserIds.Count == 0)
        {
            return new HashSet<UserId>();
        }

        var distinctActorUserIds = actorUserIds.Distinct().ToList();
        var (friendIds, roomMemberIds, overrideByUserId) = await LoadCandidateAccessContextAsync(session, ct);
        var dismissedUserIds = await _dismissals.GetDismissedViewerIdsAsync(
            [session.Id],
            distinctActorUserIds,
            ct);
        var deniedUserIds = new HashSet<UserId>();
        foreach (var actorUserId in distinctActorUserIds)
        {
            if (!TryResolveCandidateAccess(
                session,
                actorUserId,
                friendIds,
                roomMemberIds,
                overrideByUserId,
                dismissedUserIds,
                out _))
            {
                deniedUserIds.Add(actorUserId);
            }
        }

        return deniedUserIds;
    }

    public async Task<IReadOnlyList<UserId>> GetAuthorizedUserIdsAsync(
        Session session,
        CancellationToken ct = default)
    {
        var candidateUserIds = new HashSet<UserId> { session.OwnerUserId };
        var (friendIds, roomMemberIds, overrideByUserId) = await LoadCandidateAccessContextAsync(session, ct);

        foreach (var friendId in friendIds)
        {
            candidateUserIds.Add(friendId);
        }

        foreach (var roomMemberId in roomMemberIds)
        {
            candidateUserIds.Add(roomMemberId);
        }

        foreach (var actorUserId in overrideByUserId.Keys)
        {
            candidateUserIds.Add(actorUserId);
        }

        var dismissedUserIds = await _dismissals.GetDismissedViewerIdsAsync(
            [session.Id],
            candidateUserIds,
            ct);
        var authorizedUserIds = new List<UserId>(candidateUserIds.Count);
        foreach (var candidateUserId in candidateUserIds)
        {
            if (TryResolveCandidateAccess(
                session,
                candidateUserId,
                friendIds,
                roomMemberIds,
                overrideByUserId,
                dismissedUserIds,
                out _))
            {
                authorizedUserIds.Add(candidateUserId);
            }
        }

        authorizedUserIds.Sort((left, right) => left.Value.CompareTo(right.Value));
        return authorizedUserIds;
    }

    private async Task<(HashSet<UserId> FriendIds, HashSet<UserId> RoomMemberIds, Dictionary<UserId, AccessLevel> OverrideByUserId)>
        LoadCandidateAccessContextAsync(
            Session session,
            CancellationToken ct)
    {
        HashSet<UserId> friendIds = [];
        HashSet<UserId> roomMemberIds = [];

        if (session.Scope == SessionScope.Friends)
        {
            friendIds = [.. await _friendships.GetFriendIdsAsync(session.OwnerUserId, ct)];
        }
        else if (session.Scope == SessionScope.Room && session.RoomId.HasValue)
        {
            roomMemberIds = [.. await _roomMembers.GetMemberUserIdsAsync(session.RoomId.Value, ct)];
        }

        var activeOverrides = await _overrides.GetActiveBySessionAsync(session.Id, ct);
        var overrideByUserId = new Dictionary<UserId, AccessLevel>(activeOverrides.Count);
        var now = _timeProvider.GetUtcNow();
        foreach (var activeOverride in activeOverrides)
        {
            if (activeOverride.IsActiveAt(now))
            {
                overrideByUserId[activeOverride.ActorUserId] = activeOverride.AccessLevel;
            }
        }

        return (friendIds, roomMemberIds, overrideByUserId);
    }

    private static bool TryResolveCandidateAccess(
        Session session,
        UserId actorUserId,
        IReadOnlySet<UserId> friendIds,
        IReadOnlySet<UserId> roomMemberIds,
        IReadOnlyDictionary<UserId, AccessLevel> overrideByUserId,
        IReadOnlySet<UserId> dismissedUserIds,
        out AccessLevel accessLevel)
    {
        if (!session.IsOwner(actorUserId) && dismissedUserIds.Contains(actorUserId))
        {
            accessLevel = default;
            return false;
        }

        AccessLevel? overrideLevel = overrideByUserId.TryGetValue(actorUserId, out var resolvedOverride)
            ? resolvedOverride
            : null;
        return AccessResolver.TryResolveAccess(
            session.IsOwner(actorUserId),
            session.Scope,
            session.DefaultAccess,
            friendIds.Contains(actorUserId),
            roomMemberIds.Contains(actorUserId),
            overrideLevel,
            out accessLevel);
    }
}
