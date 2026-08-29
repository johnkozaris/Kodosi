using System.Text.Json;
using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;

using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

public sealed class SharedSurfaceEventPublisherTests
{
    [Fact]
    public async Task PublishSessionChangeAsync_PublishesOwnSessions_ForPrivateChanges()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var ownerUserId = UserId.New();
        var otherUserId = UserId.New();
        var ownerQueue = broadcaster.Register("owner", ownerUserId, "owner-device");
        var otherQueue = broadcaster.Register("other", otherUserId, "other-device");
        var publisher = CreatePublisher(broadcaster);
        var state = new SessionDiscoveryTarget(
            SessionId.New(),
            ownerUserId,
            SessionScope.JustMe,
            null);

        await publisher.PublishSessionChangeAsync(null, state, TestContext.Current.CancellationToken);

        var payload = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(payload);

        var message = JsonSerializer.Deserialize(
            payload!,
            WsJsonContext.Default.DiscoveryInvalidatedMessage);
        Assert.Equal([DiscoverySurface.OwnSessions], message?.Surfaces);

        using var timeoutCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => otherQueue.ReadAsync(timeoutCts.Token));
    }

    [Fact]
    public async Task PublishFriendRequestsChanged_Targets_Exact_Users()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var firstUserId = UserId.New();
        var secondUserId = UserId.New();
        var otherUserId = UserId.New();
        var firstQueue = broadcaster.Register("first", firstUserId, "first-device");
        var secondQueue = broadcaster.Register("second", secondUserId, "second-device");
        var otherQueue = broadcaster.Register("other", otherUserId, "other-device");
        var publisher = CreatePublisher(broadcaster);

        publisher.PublishFriendRequestsChanged(firstUserId, secondUserId);

        var firstPayload = await firstQueue.ReadAsync(CancellationToken.None);
        var secondPayload = await secondQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(firstPayload);
        Assert.NotNull(secondPayload);

        var firstMessage = JsonSerializer.Deserialize(
            firstPayload!,
            WsJsonContext.Default.DiscoveryInvalidatedMessage);
        var secondMessage = JsonSerializer.Deserialize(
            secondPayload!,
            WsJsonContext.Default.DiscoveryInvalidatedMessage);

        Assert.Equal([DiscoverySurface.Friends], firstMessage?.Surfaces);
        Assert.Equal([DiscoverySurface.Friends], secondMessage?.Surfaces);

        using var timeoutCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => otherQueue.ReadAsync(timeoutCts.Token));
    }

    [Fact]
    public async Task PublishRoomMemberAddedAsync_Invalidates_All_Members_Plus_Actor()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var existingMemberId = UserId.New();
        var addedMemberId = UserId.New();
        var outsiderId = UserId.New();
        var existingQueue = broadcaster.Register("existing", existingMemberId, "existing-device");
        var addedQueue = broadcaster.Register("added", addedMemberId, "added-device");
        var outsiderQueue = broadcaster.Register("outsider", outsiderId, "outsider-device");

        var roomId = RoomId.From(Guid.NewGuid());
        var members = new FakeRoomMemberRepository([existingMemberId, addedMemberId]);
        var publisher = new SharedSurfaceEventPublisher(
            new DiscoveryAudienceResolver(
                new FakeFriendshipRepository(),
                members,
                new FakeSessionViewerDismissalRepository()),
            broadcaster);

        await publisher.PublishRoomMemberAddedAsync(
            roomId,
            addedMemberId,
            TestContext.Current.CancellationToken);

        var expected = new[] { DiscoverySurface.RoomCatalog, DiscoverySurface.RoomFeed };
        AssertSurfacesEqual(expected, await existingQueue.ReadAsync(CancellationToken.None));
        AssertSurfacesEqual(expected, await addedQueue.ReadAsync(CancellationToken.None));

        using var timeoutCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => outsiderQueue.ReadAsync(timeoutCts.Token));
    }

    [Fact]
    public async Task PublishRoomMemberRemovedAsync_Invalidates_Remaining_Members_And_Removed_User()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var remainingMemberId = UserId.New();
        var removedMemberId = UserId.New();
        var outsiderId = UserId.New();
        var remainingQueue = broadcaster.Register("remaining", remainingMemberId, "remaining-device");
        var removedQueue = broadcaster.Register("removed", removedMemberId, "removed-device");
        var outsiderQueue = broadcaster.Register("outsider", outsiderId, "outsider-device");

        var roomId = RoomId.From(Guid.NewGuid());
        var members = new FakeRoomMemberRepository([remainingMemberId]);
        var publisher = new SharedSurfaceEventPublisher(
            new DiscoveryAudienceResolver(
                new FakeFriendshipRepository(),
                members,
                new FakeSessionViewerDismissalRepository()),
            broadcaster);

        await publisher.PublishRoomMemberRemovedAsync(
            roomId,
            removedMemberId,
            TestContext.Current.CancellationToken);

        var expected = new[] { DiscoverySurface.RoomCatalog, DiscoverySurface.RoomFeed };
        AssertSurfacesEqual(expected, await remainingQueue.ReadAsync(CancellationToken.None));
        AssertSurfacesEqual(expected, await removedQueue.ReadAsync(CancellationToken.None));

        using var timeoutCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(() => outsiderQueue.ReadAsync(timeoutCts.Token));
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task PublishRoomProjectionChanged_Targets_All_Active_Member_Devices(bool chat)
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var actorId = UserId.New();
        var memberId = UserId.New();
        var outsiderId = UserId.New();
        var actorFirst = broadcaster.Register("actor-1", actorId, "actor-device-1");
        var actorSecond = broadcaster.Register("actor-2", actorId, "actor-device-2");
        var memberQueue = broadcaster.Register("member", memberId, "member-device");
        var outsiderQueue = broadcaster.Register("outsider", outsiderId, "outsider-device");
        var roomId = RoomId.From(Guid.NewGuid());
        var publisher = new SharedSurfaceEventPublisher(
            new DiscoveryAudienceResolver(
                new FakeFriendshipRepository(),
                new FakeRoomMemberRepository([actorId, memberId]),
                new FakeSessionViewerDismissalRepository()),
            broadcaster);

        if (chat)
        {
            await publisher.PublishRoomChatMessageAsync(
                roomId, actorId, TestContext.Current.CancellationToken);
        }
        else
        {
            await publisher.PublishRoomTaskChangedAsync(
                roomId, actorId, TestContext.Current.CancellationToken);
        }

        var expectedSurface = chat ? DiscoverySurface.RoomChat : DiscoverySurface.RoomTasks;
        foreach (var queue in new[] { actorFirst, actorSecond, memberQueue })
        {
            var payload = await queue.ReadAsync(CancellationToken.None);
            var message = JsonSerializer.Deserialize(
                payload!, WsJsonContext.Default.DiscoveryInvalidatedMessage);
            Assert.Equal([expectedSurface], message?.Surfaces);
            Assert.Equal(roomId.Value.ToString(), message?.RoomId);
        }

        using var timeoutCts = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(
            () => outsiderQueue.ReadAsync(timeoutCts.Token));
    }

    private static void AssertSurfacesEqual(IReadOnlyList<DiscoverySurface> expected, byte[]? payload)
    {
        Assert.NotNull(payload);
        var message = JsonSerializer.Deserialize(
            payload!,
            WsJsonContext.Default.DiscoveryInvalidatedMessage);
        Assert.NotNull(message);
        Assert.Equal(
            expected.OrderBy(static s => s),
            message!.Surfaces.OrderBy(static s => s));
    }

    private static SharedSurfaceEventPublisher CreatePublisher(UserEventBroadcaster broadcaster)
    {
        return new SharedSurfaceEventPublisher(
            new DiscoveryAudienceResolver(
                new FakeFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeSessionViewerDismissalRepository()),
            broadcaster);
    }

    private sealed class FakeFriendshipRepository : IFriendshipRepository
    {
        public Task<bool> AreFriendsAsync(UserId userA, UserId userB, CancellationToken ct = default)
            => Task.FromResult(false);

        public Task<IReadOnlyList<UserId>> GetFriendIdsAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<IReadOnlyList<Friendship>> GetPendingIncomingAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<Friendship>>([]);

        public Task<IReadOnlyList<Friendship>> GetPendingOutgoingAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<Friendship>>([]);

        public Task<Friendship?> GetAsync(UserId userA, UserId userB, CancellationToken ct = default)
            => Task.FromResult<Friendship?>(null);

        public Task AddAsync(Friendship friendship, CancellationToken ct = default)
            => Task.CompletedTask;

        public void Remove(Friendship friendship) { }
    }

    private sealed class FakeRoomMemberRepository(IReadOnlyList<UserId> members) : IRoomMemberRepository
    {
        private readonly IReadOnlyList<UserId> _members = members;

        public FakeRoomMemberRepository()
            : this([])
        {
        }

        public Task<bool> IsMemberAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
            => Task.FromResult(false);

        public Task<IReadOnlyList<RoomId>> GetActiveRoomIdsForUserAsync(
            UserId userId,
            IReadOnlyList<RoomId> roomIds,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<RoomId>>([]);

        public Task<RoomMember?> GetAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
            => Task.FromResult<RoomMember?>(null);

        public Task AddAsync(RoomMember member, CancellationToken ct = default)
            => Task.CompletedTask;

        public Task<IReadOnlyList<UserId>> GetMemberUserIdsAsync(RoomId roomId, CancellationToken ct = default)
            => Task.FromResult(_members);

        public Task<IReadOnlyList<RoomMember>> GetActiveByRoomAsync(
            RoomId roomId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<RoomMember>> GetAdmissionProofPageAfterAsync(
            RoomId roomId,
            Guid? afterUserId,
            int limit,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<UserId>> GetRoomPeerUserIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }
}
