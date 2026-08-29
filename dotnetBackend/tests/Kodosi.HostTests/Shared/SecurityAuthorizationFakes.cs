using Kodosi.Application;
using Kodosi.Domain;
using System.Text.Json;

namespace Kodosi.HostTests;

internal sealed class AlwaysValidSignatureVerifier : IPopSignatureVerifier
{
    public bool Verify(
        ReadOnlySpan<byte> publicKey,
        ReadOnlySpan<byte> message,
        ReadOnlySpan<byte> signature) => true;
}

internal static class RoomRosterTestData
{
    public static byte[] Build(
        Room room,
        long generation,
        IEnumerable<UserId> members,
        string signerDeviceId = "owner-device",
        DateTimeOffset? issuedAt = null) =>
        JsonSerializer.SerializeToUtf8Bytes(new
        {
            version = 1,
            roomId = room.Id.Value.ToString(),
            generation,
            ownerUserId = room.OwnerUserId.Value.ToString(),
            memberUserIds = members.Select(member => member.Value.ToString()).ToArray(),
            signerDeviceId,
            issuedAtMs = (issuedAt ?? DateTimeOffset.UtcNow).ToUnixTimeMilliseconds(),
        });
}

internal sealed class FakeSessionRepository : SessionRepositoryStub
{
    private readonly Dictionary<SessionId, Session> _sessions = new();
    private readonly Dictionary<SessionId, int> _participantCounts = new();
    public int GetByIdForUpdateCalls { get; private set; }
    public int HostSlotReleaseCalls { get; private set; }
    public SessionId? DropAfterFirstBulkLockRead { get; set; }

    public FakeSessionRepository(Session? session = null, params Session[] additionalSessions)
    {
        if (session is not null)
        {
            _sessions[session.Id] = session;
        }
        foreach (var additionalSession in additionalSessions)
        {
            _sessions[additionalSession.Id] = additionalSession;
        }
    }

    public override Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)
    {
        _sessions.TryGetValue(id, out var session);
        return Task.FromResult(session);
    }

    public override Task<Session?> GetByIdForUpdateAsync(
        SessionId id,
        CancellationToken ct = default)
    {
        GetByIdForUpdateCalls++;
        return GetByIdAsync(id, ct);
    }

    public override Task<IReadOnlyList<Session>> GetByIdsForUpdateAsync(
        IReadOnlyCollection<SessionId> ids,
        CancellationToken ct = default)
    {
        IReadOnlyList<Session> sessions = ids.Distinct()
            .OrderBy(id => id.Value)
            .Where(_sessions.ContainsKey)
            .Select(id => _sessions[id])
            .ToList();
        if (DropAfterFirstBulkLockRead is { } sessionId)
        {
            _sessions.Remove(sessionId);
            DropAfterFirstBulkLockRead = null;
        }
        return Task.FromResult(sessions);
    }

    public override Task AddAsync(Session session, CancellationToken ct = default)
    {
        _sessions[session.Id] = session;
        return Task.CompletedTask;
    }

    public override Task UpdateAsync(Session session, CancellationToken ct = default)
    {
        _sessions[session.Id] = session;
        return Task.CompletedTask;
    }

    public override Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(UserId ownerUserId, CancellationToken ct = default)
    {
        IReadOnlyList<SessionCardProjection> projections = _sessions.Values
            .Where(s => s.OwnerUserId == ownerUserId)
            .OrderByDescending(s => s.StartedAt)
            .Select(s => new SessionCardProjection(
                s.Id.Value, s.Title, s.Scope, s.DefaultAccess, s.Status,
                s.OwnerUserId.Value, "TestUser", null, s.StartedAt, s.IncarnationId, s.ToolKind,
                s.RoomId?.Value))
            .ToList();
        return Task.FromResult(projections);
    }

    public override Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
        RoomId roomId, FeedCursor? cursor = null, int limit = 20,
        ToolKind? toolKindFilter = null, DateTimeOffset? since = null, CancellationToken ct = default)
    {
        IReadOnlyList<SessionCardProjection> projections = _sessions.Values
            .Where(session =>
                session.RoomId == roomId
                && session.Status is SessionStatus.Live or SessionStatus.Reconnecting
                && (toolKindFilter is null || session.ToolKind == toolKindFilter.Value)
                && (since is null
                    || session.StartedAt > since.Value
                    || session.LastHeartbeatAt > since.Value)
                && (cursor is null
                    || session.StartedAt < cursor.StartedAt
                    || (session.StartedAt == cursor.StartedAt
                        && session.Id.Value.CompareTo(cursor.Id) < 0)))
            .OrderByDescending(s => s.StartedAt)
            .ThenByDescending(s => s.Id.Value)
            .Take(limit + 1)
            .Select(s => new SessionCardProjection(
                s.Id.Value, s.Title, s.Scope, s.DefaultAccess, s.Status,
                s.OwnerUserId.Value, "TestUser", null, s.StartedAt, s.IncarnationId, s.ToolKind,
                s.RoomId?.Value))
            .ToList();
        return Task.FromResult(projections);
    }

    public override Task<IReadOnlyList<SessionId>> GetActiveOwnedSessionIdsAsync(
        UserId ownerUserId,
        CancellationToken ct = default) =>
        Task.FromResult<IReadOnlyList<SessionId>>(
            _sessions.Values
                .Where(session =>
                    session.OwnerUserId == ownerUserId
                    && session.Status != SessionStatus.Ended)
                .Select(session => session.Id)
                .ToList());

    public override Task<bool> TryIncrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        int maxParticipants,
        CancellationToken ct = default)
    {
        if (!_sessions.TryGetValue(sessionId, out var session)
            || session.StartedAt != expectedStartedAt
            || session.Status == SessionStatus.Ended)
        {
            return Task.FromResult(false);
        }

        var current = _participantCounts.GetValueOrDefault(sessionId);
        if (current >= maxParticipants)
        {
            return Task.FromResult(false);
        }

        _participantCounts[sessionId] = current + 1;
        return Task.FromResult(true);
    }

    public override Task<bool> TryDecrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default)
    {
        if (!_sessions.TryGetValue(sessionId, out var session)
            || session.StartedAt != expectedStartedAt
            || _participantCounts.GetValueOrDefault(sessionId) <= 0)
        {
            return Task.FromResult(false);
        }

        var current = _participantCounts.GetValueOrDefault(sessionId);
        _participantCounts[sessionId] = Math.Max(0, current - 1);
        return Task.FromResult(true);
    }

    public override Task ClearParticipantCountAsync(
        SessionId sessionId,
        CancellationToken ct = default)
    {
        _participantCounts.Remove(sessionId);
        return Task.CompletedTask;
    }

    public override async Task<bool> TryClearParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct = default)
    {
        if (!_sessions.TryGetValue(sessionId, out var session)
            || session.StartedAt != expectedStartedAt)
        {
            return false;
        }

        await ClearParticipantCountAsync(sessionId, ct);
        return true;
    }

    public override Task<bool> TrySetParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        int participantCount,
        CancellationToken ct = default)
    {
        if (!_sessions.TryGetValue(sessionId, out var session)
            || session.StartedAt != expectedStartedAt)
        {
            return Task.FromResult(false);
        }

        _participantCounts[sessionId] = participantCount;
        return Task.FromResult(true);
    }

    public override Task ReleaseHostSlotAsync(
        SessionId sessionId,
        string? connectionId,
        CancellationToken ct = default)
    {
        HostSlotReleaseCalls++;
        if (_sessions.TryGetValue(sessionId, out var session))
        {
            session.ReleaseHostSlot(connectionId);
        }

        return Task.CompletedTask;
    }

}

internal sealed class FakeFriendshipRepository(
    bool areFriends = false,
    IReadOnlyList<UserId>? friendIds = null,
    IEnumerable<IReadOnlyList<UserId>>? friendIdResponses = null) : IFriendshipRepository
{
    private readonly bool _areFriends = areFriends;
    private readonly IReadOnlyList<UserId> _friendIds = friendIds ?? [];
    private readonly Queue<IReadOnlyList<UserId>> _friendIdResponses = friendIdResponses is null
            ? new Queue<IReadOnlyList<UserId>>()
            : new Queue<IReadOnlyList<UserId>>(friendIdResponses);

    public int AreFriendsCalls { get; private set; }
    public int GetFriendIdsCalls { get; private set; }

    public Task<bool> AreFriendsAsync(UserId userA, UserId userB, CancellationToken ct = default)
    {
        AreFriendsCalls++;
        return Task.FromResult(_areFriends || _friendIds.Contains(userA) || _friendIds.Contains(userB));
    }

    public Task<IReadOnlyList<UserId>> GetFriendIdsAsync(UserId userId, CancellationToken ct = default)
    {
        GetFriendIdsCalls++;
        if (_friendIdResponses.Count > 0)
        {
            return Task.FromResult(_friendIdResponses.Dequeue());
        }

        return Task.FromResult(_friendIds);
    }

    public Task<IReadOnlyList<Friendship>> GetPendingIncomingAsync(UserId userId, CancellationToken ct = default)
        => throw new NotSupportedException();

    public Task<IReadOnlyList<Friendship>> GetPendingOutgoingAsync(UserId userId, CancellationToken ct = default)
        => throw new NotSupportedException();

    public Task<Friendship?> GetAsync(UserId userA, UserId userB, CancellationToken ct = default)
        => throw new NotSupportedException();

    public Task AddAsync(Friendship friendship, CancellationToken ct = default)
        => throw new NotSupportedException();

    public void Remove(Friendship friendship)
        => throw new NotSupportedException();
}

internal sealed class FakeRoomRepository(Room room) : IRoomRepository
{
    private readonly Room _room = room;

    public Task<Room?> GetByIdAsync(RoomId id, CancellationToken ct = default)
        => Task.FromResult<Room?>(_room.Id == id ? _room : null);

    public Task AddAsync(Room room, CancellationToken ct = default)
        => throw new NotSupportedException();

    public Task<IReadOnlyList<Room>> GetByMemberPageAsync(
        UserId userId,
        FeedCursor? cursor,
        int limit,
        CancellationToken ct = default) =>
        Task.FromResult<IReadOnlyList<Room>>(
            cursor is null && limit > 0 ? [_room] : []);

    public Task<IReadOnlyList<Room>> GetByIdsAsync(
        IReadOnlyCollection<RoomId> ids,
        CancellationToken ct = default)
        => throw new NotSupportedException();
}

internal sealed class FakeRoomMemberRepository : IRoomMemberRepository
{
    private readonly Dictionary<(RoomId RoomId, UserId UserId), RoomMember> _members = [];

    public FakeRoomMemberRepository()
    {
    }

    public FakeRoomMemberRepository(params (RoomId RoomId, UserId UserId)[] members)
        : this(members.Select(member => (member.RoomId, member.UserId, RoomRole.Member)).ToArray())
    {
    }

    public FakeRoomMemberRepository(params (RoomId RoomId, UserId UserId, RoomRole Role)[] members)
    {
        foreach (var member in members)
        {
            _members[(member.RoomId, member.UserId)] = DomainFixtureHydrator.RoomMember(
                member.RoomId,
                member.UserId,
                member.Role);
        }
    }

    public int IsMemberCalls { get; private set; }
    public int GetMemberUserIdsCalls { get; private set; }

    public Task<bool> IsMemberAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
    {
        IsMemberCalls++;
        return Task.FromResult(
            _members.TryGetValue((roomId, userId), out var member)
            && member.IsActive);
    }

    public Task<IReadOnlyList<RoomId>> GetActiveRoomIdsForUserAsync(
        UserId userId,
        IReadOnlyList<RoomId> roomIds,
        CancellationToken ct = default)
    {
        IReadOnlyList<RoomId> result = _members.Values
            .Where(member =>
                member.UserId == userId
                && member.IsActive
                && roomIds.Contains(member.RoomId))
            .Select(member => member.RoomId)
            .Distinct()
            .ToList();
        return Task.FromResult(result);
    }

    public Task<RoomMember?> GetAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
    {
        _members.TryGetValue((roomId, userId), out var member);
        return Task.FromResult<RoomMember?>(member);
    }

    public Task AddAsync(RoomMember member, CancellationToken ct = default)
    {
        _members[(member.RoomId, member.UserId)] = member;
        return Task.CompletedTask;
    }

    public Task<IReadOnlyList<UserId>> GetMemberUserIdsAsync(
        RoomId roomId, CancellationToken ct = default)
    {
        GetMemberUserIdsCalls++;
        IReadOnlyList<UserId> result = _members.Values
            .Where(member => member.RoomId == roomId && member.IsActive)
            .Select(member => member.UserId)
            .ToList();
        return Task.FromResult(result);
    }

    public Task<IReadOnlyList<RoomMember>> GetActiveByRoomAsync(
        RoomId roomId,
        CancellationToken ct = default)
    {
        IReadOnlyList<RoomMember> result = _members.Values
            .Where(member => member.RoomId == roomId && member.IsActive)
            .ToList();
        return Task.FromResult(result);
    }

    public Task<IReadOnlyList<RoomMember>> GetAdmissionProofPageAfterAsync(
        RoomId roomId,
        Guid? afterUserId,
        int limit,
        CancellationToken ct = default)
    {
        var cursor = afterUserId ?? Guid.Empty;
        return Task.FromResult<IReadOnlyList<RoomMember>>(_members.Values
            .Where(member =>
                member.RoomId == roomId
                && member.IsActive
                && member.Role != RoomRole.Owner
                && member.UserId.Value.CompareTo(cursor) > 0)
            .OrderBy(member => member.UserId)
            .Take(limit)
            .ToList());
    }

    public int RoomPeerLookupCalls { get; private set; }

    public Task<IReadOnlyList<UserId>> GetRoomPeerUserIdsAsync(
        UserId userId,
        CancellationToken ct = default)
    {
        RoomPeerLookupCalls++;
        IReadOnlyList<UserId> peers = _members.Values
            .Where(member => member.UserId == userId && member.IsActive)
            .SelectMany(membership => _members.Values
                .Where(peer => peer.RoomId == membership.RoomId
                    && peer.IsActive
                    && peer.UserId != userId)
                .Select(peer => peer.UserId))
            .Distinct()
            .ToList();
        return Task.FromResult(peers);
    }
}

internal sealed class FakeUserRepository : IUserRepository
{
    private readonly Dictionary<UserId, User> _users = new();

    public FakeUserRepository(params User[] users)
    {
        foreach (var user in users)
        {
            _users[user.Id] = user;
        }
    }

    public Task<User?> GetByIdAsync(UserId id, CancellationToken ct = default)
    {
        _users.TryGetValue(id, out var user);
        return Task.FromResult(user);
    }

    public Task<User?> GetByHandleAsync(string handle, CancellationToken ct = default)
        => throw new NotSupportedException();

    public Task<IReadOnlyList<string>> GetHandlesByPrefixAsync(string prefix, CancellationToken ct = default)
        => throw new NotSupportedException();

    public Task<IReadOnlyList<User>> GetByIdsAsync(IReadOnlyList<UserId> ids, CancellationToken ct = default)
    {
        IReadOnlyList<User> result = ids
            .Where(id => _users.ContainsKey(id))
            .Select(id => _users[id])
            .ToList();
        return Task.FromResult(result);
    }

    public Task AddAsync(User user, CancellationToken ct = default)
    {
        _users[user.Id] = user;
        return Task.CompletedTask;
    }
}

internal sealed class FakeAccessOverrideAuditRepository : IAccessOverrideAuditRepository
{
    public List<AccessOverrideAuditEntry> Entries { get; } = new();

    public Task AddAsync(AccessOverrideAuditEntry entry, CancellationToken ct = default)
    {
        Entries.Add(entry);
        return Task.CompletedTask;
    }
}

internal sealed class FakeAccessOverrideRepository : IAccessOverrideRepository
{
    private readonly Dictionary<(SessionId SessionId, UserId ActorId), SessionAccessOverride> _overrides = new();
    private readonly Dictionary<SessionId, UserId> _sessionOwners = new();

    public FakeAccessOverrideRepository(params SessionAccessOverride[] overrides)
    {
        foreach (var accessOverride in overrides)
        {
            _overrides[(accessOverride.SessionId, accessOverride.ActorUserId)] = accessOverride;
        }
    }

    public FakeAccessOverrideRepository WithSessionOwner(SessionId sessionId, UserId ownerId)
    {
        _sessionOwners[sessionId] = ownerId;
        return this;
    }

    public int GetActiveAsyncCalls { get; private set; }
    public int GetActiveBySessionCalls { get; private set; }

    public Task<SessionAccessOverride?> GetActiveAsync(
        SessionId sessionId,
        UserId actorUserId,
        CancellationToken ct = default)
    {
        GetActiveAsyncCalls++;
        _overrides.TryGetValue((sessionId, actorUserId), out var accessOverride);
        return Task.FromResult(accessOverride?.IsActiveAt(DateTimeOffset.UtcNow) == true ? accessOverride : null);
    }

    public Task<SessionAccessOverride?> GetAsync(
        SessionId sessionId,
        UserId actorUserId,
        CancellationToken ct = default)
    {
        _overrides.TryGetValue((sessionId, actorUserId), out var accessOverride);
        return Task.FromResult(accessOverride);
    }

    public Task<IReadOnlyList<SessionAccessOverride>> GetActiveForActorAsync(
        UserId actorUserId,
        IReadOnlyList<SessionId> sessionIds,
        CancellationToken ct = default)
    {
        IReadOnlyList<SessionAccessOverride> activeOverrides = _overrides.Values
            .Where(accessOverride =>
                accessOverride.ActorUserId == actorUserId
                && sessionIds.Contains(accessOverride.SessionId)
                && accessOverride.IsActiveAt(DateTimeOffset.UtcNow))
            .ToList();
        return Task.FromResult(activeOverrides);
    }

    public Task<IReadOnlyList<SessionAccessOverride>> GetActiveBySessionAsync(
        SessionId sessionId,
        CancellationToken ct = default)
    {
        GetActiveBySessionCalls++;
        IReadOnlyList<SessionAccessOverride> activeOverrides = _overrides.Values
            .Where(accessOverride =>
                accessOverride.SessionId == sessionId
                && accessOverride.IsActiveAt(DateTimeOffset.UtcNow))
            .ToList();
        return Task.FromResult(activeOverrides);
    }

    public Task<IReadOnlyList<SessionAccessOverride>> GetUnrevokedBySessionAsync(
        SessionId sessionId,
        CancellationToken ct = default)
    {
        IReadOnlyList<SessionAccessOverride> unrevoked = _overrides.Values
            .Where(accessOverride =>
                accessOverride.SessionId == sessionId
                && accessOverride.RevokedAt is null)
            .ToList();
        return Task.FromResult(unrevoked);
    }

    public Task AddAsync(SessionAccessOverride accessOverride, CancellationToken ct = default)
    {
        _overrides[(accessOverride.SessionId, accessOverride.ActorUserId)] = accessOverride;
        return Task.CompletedTask;
    }

    public Task UpdateAsync(SessionAccessOverride accessOverride, CancellationToken ct = default)
    {
        _overrides[(accessOverride.SessionId, accessOverride.ActorUserId)] = accessOverride;
        return Task.CompletedTask;
    }

    public Task UpdateRangeAsync(
        IReadOnlyCollection<SessionAccessOverride> accessOverrides,
        CancellationToken ct = default)
    {
        foreach (var accessOverride in accessOverrides)
        {
            _overrides[(accessOverride.SessionId, accessOverride.ActorUserId)] = accessOverride;
        }

        return Task.CompletedTask;
    }

    public Task<IReadOnlyList<UserId>> GetActiveRelatedUserIdsAsync(
        UserId userId,
        CancellationToken ct = default)
    {
        IReadOnlyList<UserId> relatedUserIds = _overrides.Values
            .Where(accessOverride =>
                accessOverride.IsActiveAt(DateTimeOffset.UtcNow)
                && _sessionOwners.ContainsKey(accessOverride.SessionId))
            .Select(accessOverride =>
            {
                var owner = _sessionOwners[accessOverride.SessionId];
                return owner == userId
                    ? accessOverride.ActorUserId
                    : accessOverride.ActorUserId == userId
                        ? owner
                        : (UserId?)null;
            })
            .OfType<UserId>()
            .Where(relatedUserId => relatedUserId != userId)
            .Distinct()
            .OrderBy(relatedUserId => relatedUserId.Value)
            .ToList();
        return Task.FromResult(relatedUserIds);
    }

    public int HasActiveRelationshipCalls { get; private set; }

    public Task<bool> HasActiveRelationshipAsync(UserId userA, UserId userB, CancellationToken ct = default)
    {
        HasActiveRelationshipCalls++;
        if (userA == userB)
        {
            return Task.FromResult(false);
        }

        var match = _overrides.Values.Any(o =>
            o.IsActiveAt(DateTimeOffset.UtcNow)
            && _sessionOwners.TryGetValue(o.SessionId, out var owner)
            && ((owner == userA && o.ActorUserId == userB)
                || (owner == userB && o.ActorUserId == userA)));
        return Task.FromResult(match);
    }

    public Task<IReadOnlyList<SessionAccessOverride>> GetExpiredUnrevokedAsync(
        DateTimeOffset now,
        int limit,
        CancellationToken ct = default)
    {
        IReadOnlyList<SessionAccessOverride> expired = _overrides.Values
            .Where(accessOverride =>
                accessOverride.RevokedAt is null
                && accessOverride.ExpiresAt <= now)
            .OrderBy(accessOverride => accessOverride.ExpiresAt)
            .Take(Math.Clamp(limit, 1, 500))
            .ToList();
        return Task.FromResult(expired);
    }

    public Task<bool> TryRevokeExpiredAsync(
        SessionId sessionId,
        UserId actorUserId,
        DateTimeOffset observedExpiry,
        DateTimeOffset revokedAt,
        CancellationToken ct = default)
    {
        if (!_overrides.TryGetValue(
                (sessionId, actorUserId),
                out var accessOverride)
            || accessOverride.RevokedAt is not null
            || accessOverride.ExpiresAt != observedExpiry
            || observedExpiry > revokedAt)
        {
            return Task.FromResult(false);
        }

        accessOverride.Revoke(revokedAt);
        return Task.FromResult(true);
    }
}

internal sealed class FakeSessionViewerDismissalRepository(
    params SessionViewerDismissal[] dismissals)
    : ISessionViewerDismissalRepository
{
    private readonly Dictionary<(SessionId SessionId, UserId ViewerUserId), SessionViewerDismissal>
        _dismissals = dismissals.ToDictionary(
            dismissal => (dismissal.SessionId, dismissal.ViewerUserId));

    public int ExistsCalls { get; private set; }
    public int DismissedViewerBatchCalls { get; private set; }
    public int DismissedSessionBatchCalls { get; private set; }
    public int AddCalls { get; private set; }
    public int Count => _dismissals.Count;

    public Task<bool> ExistsAsync(
        SessionId sessionId,
        UserId viewerUserId,
        CancellationToken ct = default)
    {
        ExistsCalls++;
        return Task.FromResult(_dismissals.ContainsKey((sessionId, viewerUserId)));
    }

    public Task<IReadOnlySet<UserId>> GetDismissedViewerIdsAsync(
        IReadOnlyCollection<SessionId> sessionIds,
        IReadOnlyCollection<UserId> viewerUserIds,
        CancellationToken ct = default)
    {
        DismissedViewerBatchCalls++;
        IReadOnlySet<UserId> result = _dismissals.Values
            .Where(dismissal =>
                sessionIds.Contains(dismissal.SessionId)
                && viewerUserIds.Contains(dismissal.ViewerUserId))
            .Select(dismissal => dismissal.ViewerUserId)
            .ToHashSet();
        return Task.FromResult(result);
    }

    public Task<IReadOnlySet<SessionId>> GetDismissedSessionIdsAsync(
        UserId viewerUserId,
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct = default)
    {
        DismissedSessionBatchCalls++;
        IReadOnlySet<SessionId> result = _dismissals.Values
            .Where(dismissal =>
                dismissal.ViewerUserId == viewerUserId
                && sessionIds.Contains(dismissal.SessionId))
            .Select(dismissal => dismissal.SessionId)
            .ToHashSet();
        return Task.FromResult(result);
    }

    public Task<bool> AddIfAbsentAsync(
        SessionViewerDismissal dismissal,
        CancellationToken ct = default)
    {
        AddCalls++;
        var added = _dismissals.TryAdd(
            (dismissal.SessionId, dismissal.ViewerUserId),
            dismissal);
        return Task.FromResult(added);
    }

    public Task DeleteForSessionAsync(
        SessionId sessionId,
        CancellationToken ct = default)
    {
        foreach (var key in _dismissals.Keys.Where(key => key.SessionId == sessionId).ToList())
        {
            _dismissals.Remove(key);
        }
        return Task.CompletedTask;
    }
}

internal sealed class FakeUserDeviceListRepository : IUserDeviceListRepository
{
    private readonly Dictionary<UserId, UserDeviceList> _byUser = new();

    public FakeUserDeviceListRepository(params UserDeviceList[] lists)
    {
        foreach (var list in lists)
        {
            if (!_byUser.TryGetValue(list.UserId, out var existing)
                || list.Generation > existing.Generation)
            {
                _byUser[list.UserId] = list;
            }
        }
    }

    public Task<UserDeviceList?> GetLatestAsync(UserId userId, CancellationToken ct = default)
    {
        _byUser.TryGetValue(userId, out var list);
        return Task.FromResult(list);
    }

    public Task<UserDeviceList?> GetGenerationAsync(
        UserId userId,
        long generation,
        CancellationToken ct = default)
    {
        _byUser.TryGetValue(userId, out var list);
        return Task.FromResult(list?.Generation == generation ? list : null);
    }

    public Task<IReadOnlyList<UserDeviceList>> GetLatestByUserIdsAsync(
        IReadOnlyCollection<UserId> userIds,
        CancellationToken ct = default)
    {
        IReadOnlyList<UserDeviceList> lists = userIds
            .Select(userId => _byUser.GetValueOrDefault(userId))
            .Where(list => list is not null)
            .Cast<UserDeviceList>()
            .ToList();
        return Task.FromResult(lists);
    }

    public Task AddAsync(UserDeviceList list, CancellationToken ct = default)
    {
        if (!_byUser.TryGetValue(list.UserId, out var existing)
            || list.Generation > existing.Generation)
        {
            _byUser[list.UserId] = list;
        }
        return Task.CompletedTask;
    }

    public Task<int> RemoveAllForUserAsync(UserId userId, CancellationToken ct = default)
    {
        var removed = _byUser.Remove(userId) ? 1 : 0;
        return Task.FromResult(removed);
    }
}

internal sealed class FakeUserDeviceRepository : IUserDeviceRepository
{
    private readonly Dictionary<string, UserDevice> _devices = new(StringComparer.Ordinal);

    public FakeUserDeviceRepository(params UserDevice[] devices)
    {
        foreach (var device in devices)
        {
            _devices[device.DeviceId] = device;
        }
    }

    public Task<IReadOnlyList<UserDevice>> GetByUserIdAsync(UserId userId, CancellationToken ct = default)
    {
        IReadOnlyList<UserDevice> devices = _devices.Values
            .Where(device => device.UserId == userId)
            .ToList();
        return Task.FromResult(devices);
    }

    public Task<UserDevice?> GetByDeviceIdAsync(string deviceId, CancellationToken ct = default)
    {
        _devices.TryGetValue(deviceId, out var device);
        return Task.FromResult(device);
    }

    public Task<IReadOnlyDictionary<string, UserId>>
        GetUserIdsByDeviceIdsAsync(
            IReadOnlyCollection<string> deviceIds,
            CancellationToken ct = default)
    {
        IReadOnlyDictionary<string, UserId> usersByDevice = deviceIds
            .Distinct(StringComparer.Ordinal)
            .Where(_devices.ContainsKey)
            .ToDictionary(
                deviceId => deviceId,
                deviceId => _devices[deviceId].UserId,
                StringComparer.Ordinal);
        return Task.FromResult(usersByDevice);
    }

    public Task<IReadOnlyList<UserDevice>> GetByUserIdsAsync(IReadOnlyList<UserId> userIds, CancellationToken ct = default)
    {
        IReadOnlyList<UserDevice> devices = _devices.Values
            .Where(device => userIds.Contains(device.UserId))
            .ToList();
        return Task.FromResult(devices);
    }

    public Task AddAsync(UserDevice device, CancellationToken ct = default)
    {
        _devices[device.DeviceId] = device;
        return Task.CompletedTask;
    }

    public void Update(UserDevice device)
    {
        _devices[device.DeviceId] = device;
    }

    public Task<int> RemoveAllForUserAsync(UserId userId, CancellationToken ct = default)
    {
        var toRemove = _devices.Values.Where(d => d.UserId == userId).ToList();
        foreach (var device in toRemove)
        {
            _devices.Remove(device.DeviceId);
        }
        return Task.FromResult(toRemove.Count);
    }
}

internal sealed class FakeSessionKeyBlobRepository : ISessionKeyBlobRepository
{
    private readonly Dictionary<(SessionId SessionId, string DeviceId), SessionKeyBlob> _blobs = new();

    public FakeSessionKeyBlobRepository(params SessionKeyBlob[] blobs)
    {
        foreach (var blob in blobs)
        {
            _blobs[(blob.SessionId, blob.RecipientDeviceId)] = blob;
        }
    }

    public Task<SessionKeyBlob?> GetForDeviceAsync(
        SessionId sessionId,
        string recipientDeviceId,
        CancellationToken ct = default)
    {
        _blobs.TryGetValue((sessionId, recipientDeviceId), out var blob);
        return Task.FromResult(blob);
    }

    public Task AddRangeAsync(IReadOnlyList<SessionKeyBlob> blobs, CancellationToken ct = default)
    {
        foreach (var blob in blobs)
        {
            _blobs[(blob.SessionId, blob.RecipientDeviceId)] = blob;
        }

        return Task.CompletedTask;
    }

    public Task DeleteForSessionAsync(SessionId sessionId, CancellationToken ct = default)
    {
        RemoveForSession(sessionId);
        return Task.CompletedTask;
    }

    public void RemoveForSession(SessionId sessionId)
    {
        foreach (var key in _blobs.Keys.Where(key => key.SessionId == sessionId).ToList())
        {
            _blobs.Remove(key);
        }
    }

    public Task<IReadOnlyList<DeviceRevocationSessionTarget>>
        GetSessionTargetsForRecipientDevicesAsync(
            IReadOnlyCollection<string> recipientDeviceIds,
            CancellationToken ct = default) =>
        throw new NotSupportedException();

    public Task<IReadOnlyList<SessionId>> GetSessionIdsForRecipientDevicesAsync(
        IReadOnlyCollection<string> recipientDeviceIds,
        CancellationToken ct = default)
    {
        IReadOnlyList<SessionId> sessionIds = _blobs.Keys
            .Where(key => recipientDeviceIds.Contains(key.DeviceId))
            .Select(key => key.SessionId)
            .Distinct()
            .ToList();
        return Task.FromResult(sessionIds);
    }

    public Task<int> DeleteForRecipientDevicesAsync(
        IReadOnlyCollection<string> recipientDeviceIds,
        CancellationToken ct = default)
    {
        var removed = 0;
        foreach (var key in _blobs.Keys
            .Where(key => recipientDeviceIds.Contains(key.DeviceId))
            .ToList())
        {
            _blobs.Remove(key);
            removed++;
        }

        return Task.FromResult(removed);
    }
}

internal sealed class FakeSessionEndMutationRepository : ISessionEndMutationRepository
{
    private readonly Dictionary<(UserId OwnerUserId, Guid MutationId), SessionEndMutation>
        _mutations = [];

    public IReadOnlyCollection<SessionEndMutation> Mutations => _mutations.Values;

    public Task AcquireAsync(
        UserId ownerUserId,
        Guid mutationId,
        CancellationToken ct = default) =>
        Task.CompletedTask;

    public Task<SessionEndMutation?> GetAsync(
        UserId ownerUserId,
        Guid mutationId,
        CancellationToken ct = default)
    {
        _mutations.TryGetValue((ownerUserId, mutationId), out var mutation);
        return Task.FromResult(mutation);
    }

    public Task AddAsync(
        SessionEndMutation mutation,
        CancellationToken ct = default)
    {
        if (!_mutations.TryAdd(
                (mutation.OwnerUserId, mutation.MutationId),
                mutation))
        {
            throw new InvalidOperationException("Duplicate session end mutation.");
        }
        return Task.CompletedTask;
    }
}

internal sealed class FakeUnitOfWork : UnitOfWorkStub
{
    public int SaveChangesCalls { get; private set; }

    public override Task SaveChangesAsync(CancellationToken ct = default)
    {
        SaveChangesCalls++;
        return Task.CompletedTask;
    }

    public override Task<ITransactionScope> BeginTransactionAsync(
        CancellationToken ct = default) =>
        Task.FromResult<ITransactionScope>(new CompletedTransactionScope());
}

internal sealed class FakeSessionEndAuthority : ISessionEndAuthority
{
    public bool LeaseHeld { get; private set; }
    public IReadOnlyList<SessionId> AcquiredSessionIds { get; private set; } = [];
    public List<(SessionDiscoveryTarget Session, DateTimeOffset StartedAt)>
        RetiredIncarnations
    { get; } = [];

    public ValueTask<IAsyncDisposable> AcquireAsync(
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct = default)
    {
        AcquiredSessionIds = sessionIds
            .Distinct()
            .OrderBy(sessionId => sessionId.Value)
            .ToList();
        LeaseHeld = true;
        return ValueTask.FromResult<IAsyncDisposable>(new Lease(this));
    }

    public Task ProjectCommittedAsync(
        IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
        CancellationToken ct = default) =>
        Task.CompletedTask;

    public Task RetireEndedIncarnationAsync(
        SessionDiscoveryTarget endedSession,
        DateTimeOffset startedAt,
        CancellationToken ct = default)
    {
        RetiredIncarnations.Add((endedSession, startedAt));
        return Task.CompletedTask;
    }

    private sealed class Lease(FakeSessionEndAuthority owner) : IAsyncDisposable
    {
        public ValueTask DisposeAsync()
        {
            owner.LeaseHeld = false;
            return ValueTask.CompletedTask;
        }
    }
}

internal sealed class FakeRoomLifecycleLock : IRoomLifecycleLock
{
    public Task AcquireAsync(
        RoomId roomId,
        CancellationToken ct = default) =>
        Task.CompletedTask;
}

internal sealed class FakeSessionIncarnationRepository
    : ISessionIncarnationRepository
{
    private readonly List<SessionIncarnationRecord> _records = [];

    public FakeSessionIncarnationRepository(Session? existing = null)
    {
        if (existing is not null)
        {
            _records.Add(new SessionIncarnationRecord(
                existing.Id,
                existing.IncarnationGeneration,
                existing.IncarnationId,
                existing.IncarnationProtocolVersion,
                existing.IncarnationId,
                existing.StartedAt));
        }
    }

    public Task<SessionIncarnationRecord?> GetByIdempotencyKeyAsync(
        SessionId sessionId,
        Guid idempotencyKey,
        CancellationToken ct = default) =>
        Task.FromResult(_records.SingleOrDefault(record =>
            record.SessionId == sessionId
            && record.IdempotencyKey == idempotencyKey));

    public Task AddAsync(
        SessionIncarnationRecord incarnation,
        CancellationToken ct = default)
    {
        _records.Add(incarnation);
        return Task.CompletedTask;
    }
}

internal sealed class FakeUserLifecycleLock : IUserLifecycleLock
{
    public List<UserId> AcquiredUserIds { get; } = [];

    public Task AcquireAsync(UserId userId, CancellationToken ct = default)
    {
        AcquiredUserIds.Add(userId);
        return Task.CompletedTask;
    }
}

internal sealed class FakeRecipientDeviceLifecycleLock
    : IRecipientDeviceLifecycleLock
{
    public IReadOnlyList<UserId> AcquiredUserIds { get; private set; } = [];

    public Task AcquireAsync(
        IReadOnlyCollection<UserId> recipientUserIds,
        CancellationToken ct = default)
    {
        AcquiredUserIds = recipientUserIds
            .Distinct()
            .OrderBy(userId => userId.Value)
            .ToList();
        return Task.CompletedTask;
    }
}

internal sealed class FakeRuntimeDirectory : ILiveSessionStateDirectory
{
    public LiveSessionPorts GetOrCreate(SessionId sessionId) => throw new NotSupportedException();
    public LiveSessionPorts? TryGet(SessionId sessionId) => null;
    public bool TryClaimHost(
        SessionId sessionId,
        string connectionId,
        CancellationTokenSource hostLifetime,
        out LiveSessionPorts? ports,
        out LiveSessionCreationOwnership? creationOwnership)
    {
        ports = null;
        creationOwnership = null;
        throw new NotSupportedException();
    }
    public void Remove(SessionId sessionId) { }
    public bool RemoveIfSame(SessionId sessionId, LiveSessionPorts expected) => false;
    public bool RemoveIfOwned(
        SessionId sessionId,
        LiveSessionCreationOwnership creationOwnership) => false;
    public IReadOnlyList<SessionId> GetActiveSessions() => [];
}
