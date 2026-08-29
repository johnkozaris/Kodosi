using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class SharedSurfaceEventPublisher(
    DiscoveryAudienceResolver audiences,
    UserEventBroadcaster broadcaster)
{
    private readonly DiscoveryAudienceResolver _audiences = audiences;
    private readonly UserEventBroadcaster _broadcaster = broadcaster;

    public async Task PublishSessionChangeAsync(
        SessionDiscoveryTarget? before,
        SessionDiscoveryTarget? after,
        CancellationToken ct = default)
    {
        var surfaces = ResolveSessionSurfaces(before, after);
        if (surfaces.Count > 0)
        {
            var audience = await _audiences.ResolveSessionChangeAsync(before, after, ct);
            _broadcaster.PublishInvalidation(audience, surfaces);
        }

        var ownerUserIds = ResolveOwnerUserIds(before, after);
        if (ownerUserIds.Count > 0)
        {
            _broadcaster.PublishInvalidation(
                _audiences.ResolveExactUsers([.. ownerUserIds]),
                new HashSet<DiscoverySurface> { DiscoverySurface.OwnSessions });
        }
    }

    public Task PublishSessionLifecycleChangedAsync(
        SessionDiscoveryTarget? state,
        CancellationToken ct = default)
    {
        return state is null
            ? Task.CompletedTask
            : PublishSessionChangeAsync(state, state, ct);
    }

    public void PublishSessionDismissed(
        UserId viewerUserId,
        SessionDiscoveryTarget session)
    {
        var surfaces = ResolveSessionSurfaces(session, session);
        _broadcaster.PublishInvalidation(
            _audiences.ResolveExactUsers(viewerUserId),
            surfaces);
    }

    public void PublishRoomCreated(UserId ownerUserId)
    {
        _broadcaster.PublishInvalidation(
            _audiences.ResolveExactUsers(ownerUserId),
            new HashSet<DiscoverySurface> { DiscoverySurface.RoomCatalog });
    }


    public Task PublishRoomMemberAddedAsync(
        RoomId roomId,
        UserId addedUserId,
        CancellationToken ct = default)
        => PublishRoomMembershipChangedAsync(roomId, addedUserId, ct);

    public Task PublishRoomMemberRemovedAsync(
        RoomId roomId,
        UserId removedUserId,
        CancellationToken ct = default)
        => PublishRoomMembershipChangedAsync(roomId, removedUserId, ct);

    private async Task PublishRoomMembershipChangedAsync(
        RoomId roomId,
        UserId changedUserId,
        CancellationToken ct)
    {
        var audience = await _audiences.ResolveRoomMembersAsync(roomId, changedUserId, ct);
        _broadcaster.PublishInvalidation(
            audience,
            new HashSet<DiscoverySurface>
            {
                DiscoverySurface.RoomCatalog,
                DiscoverySurface.RoomFeed,
            });
    }

    public void PublishFriendshipAccepted(UserId userId, UserId friendUserId)
        => PublishFriendSurfacesChanged(userId, friendUserId);

    public void PublishFriendshipRemoved(UserId userId, UserId formerFriendUserId)
        => PublishFriendSurfacesChanged(userId, formerFriendUserId);

    public void PublishFriendRequestsChanged(params UserId[] userIds)
        => PublishFriendSurfacesChanged(userIds);

    public void PublishRoomInvitationsChanged(params UserId[] userIds)
    {
        _broadcaster.PublishInvalidation(
            _audiences.ResolveExactUsers(userIds),
            new HashSet<DiscoverySurface> { DiscoverySurface.RoomCatalog });
    }



    public async Task PublishRoomChatMessageAsync(
        RoomId roomId,
        UserId actorUserId,
        CancellationToken ct = default)
    {
        var audience = await _audiences.ResolveRoomMembersAsync(roomId, actorUserId, ct);
        _broadcaster.PublishInvalidation(
            audience,
            new HashSet<DiscoverySurface> { DiscoverySurface.RoomChat },
            roomId);
    }


    public async Task PublishRoomTaskChangedAsync(
        RoomId roomId,
        UserId actorUserId,
        CancellationToken ct = default)
    {
        var audience = await _audiences.ResolveRoomMembersAsync(roomId, actorUserId, ct);
        _broadcaster.PublishInvalidation(
            audience,
            new HashSet<DiscoverySurface> { DiscoverySurface.RoomTasks },
            roomId);
    }

    private void PublishFriendSurfacesChanged(params UserId[] userIds)
    {
        _broadcaster.PublishInvalidation(
            _audiences.ResolveExactUsers(userIds),
            new HashSet<DiscoverySurface> { DiscoverySurface.Friends });
    }

    private static IReadOnlySet<UserId> ResolveOwnerUserIds(
        SessionDiscoveryTarget? before,
        SessionDiscoveryTarget? after)
    {
        var ownerUserIds = new HashSet<UserId>();
        if (before is not null)
        {
            ownerUserIds.Add(before.OwnerUserId);
        }

        if (after is not null)
        {
            ownerUserIds.Add(after.OwnerUserId);
        }

        return ownerUserIds;
    }

    private static IReadOnlySet<DiscoverySurface> ResolveSessionSurfaces(
        SessionDiscoveryTarget? before,
        SessionDiscoveryTarget? after)
    {
        var surfaces = new HashSet<DiscoverySurface>();
        AddSessionSurfaces(surfaces, before);
        AddSessionSurfaces(surfaces, after);
        return surfaces;
    }

    private static void AddSessionSurfaces(
        HashSet<DiscoverySurface> surfaces,
        SessionDiscoveryTarget? state)
    {
        if (state is null)
        {
            return;
        }

        switch (state.Scope)
        {
            case Domain.SessionScope.JustMe:
                return;
            case Domain.SessionScope.MyDevices:
                surfaces.Add(DiscoverySurface.OwnSessions);
                return;
            case Domain.SessionScope.Friends:
                surfaces.Add(DiscoverySurface.Friends);
                return;
            case Domain.SessionScope.Room:
                surfaces.Add(DiscoverySurface.RoomFeed);
                return;
            default:
                throw new InvalidOperationException($"Unsupported session scope '{state.Scope}'.");
        }
    }
}
