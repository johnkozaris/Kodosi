using Kodosi.Application;
using Kodosi.Domain;

using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class SessionReaderAuthorizationTests
{
    [Fact]
    public async Task ScopeChange_Identifies_Viewers_Who_Lose_Access()
    {
        var ownerId = UserId.New();
        var friendId = UserId.New();
        var outsiderId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var friendships = new FakeFriendshipRepository(friendIds: [friendId]);
        var service = CreateSessionReader(
            session,
            friendshipRepository: friendships,
            roomMemberRepository: new FakeRoomMemberRepository());

        var deniedViewerIds = await service.GetViewerIdsWithoutAccessAsync(
            session.Id,
            ownerId,
            [friendId, outsiderId],
            TestContext.Current.CancellationToken);

        Assert.DoesNotContain(friendId, deniedViewerIds);
        Assert.Contains(outsiderId, deniedViewerIds);
    }

    [Fact]
    public async Task GetViewerIdsWithoutAccessAsync_Batches_Friend_And_Override_Lookups()
    {
        var ownerId = UserId.New();
        var friendId = UserId.New();
        var explicitlyAllowedId = UserId.New();
        var deniedId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var friendships = new FakeFriendshipRepository(friendIds: [friendId]);
        var overrides = new FakeAccessOverrideRepository(
            SessionAccessOverride.Create(session.Id, explicitlyAllowedId, AccessLevel.Inject, ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow));
        var roomMembers = new FakeRoomMemberRepository();
        var service = CreateSessionReader(
            session,
            friendshipRepository: friendships,
            roomMemberRepository: roomMembers,
            accessOverrideRepository: overrides);

        var deniedViewerIds = await service.GetViewerIdsWithoutAccessAsync(
            session.Id,
            ownerId,
            [friendId, explicitlyAllowedId, deniedId, deniedId],
            TestContext.Current.CancellationToken);

        Assert.Equal(1, friendships.GetFriendIdsCalls);
        Assert.Equal(0, friendships.AreFriendsCalls);
        Assert.Equal(1, overrides.GetActiveBySessionCalls);
        Assert.Equal(0, overrides.GetActiveAsyncCalls);
        Assert.Equal(0, roomMembers.GetMemberUserIdsCalls);
        Assert.Equal(0, roomMembers.IsMemberCalls);
        Assert.DoesNotContain(friendId, deniedViewerIds);
        Assert.DoesNotContain(explicitlyAllowedId, deniedViewerIds);
        Assert.Equal([deniedId], deniedViewerIds);
    }

}
