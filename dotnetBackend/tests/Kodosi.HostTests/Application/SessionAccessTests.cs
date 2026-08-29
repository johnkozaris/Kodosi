using Kodosi.Application;
using Kodosi.Domain;

using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class SessionAccessTests
{
    [Fact]
    public async Task SessionDetail_Returns_For_Friend_With_Scope_Access()
    {
        var ownerId = UserId.New();
        var friendId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);

        var service = CreateSessionReader(
            session,
            friendshipRepository: new FakeFriendshipRepository(areFriends: true),
            roomMemberRepository: new FakeRoomMemberRepository());

        var result = await service.GetByIdAsync(session.Id, friendId, TestContext.Current.CancellationToken);

        Assert.Equal(session.Id.Value, result.Id);
        Assert.Equal(SessionScope.Friends, result.Scope);
        Assert.Equal(session.DefaultAccess, result.EffectiveAccess);
    }

    [Fact]
    public async Task SessionDetail_Returns_EffectiveAccess_For_Explicit_Override()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var overrides = new FakeAccessOverrideRepository(
            SessionAccessOverride.Create(session.Id, actorId, AccessLevel.Inject, ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow));

        var service = CreateSessionReader(
            session,
            friendshipRepository: new FakeFriendshipRepository(),
            roomMemberRepository: new FakeRoomMemberRepository(),
            accessOverrideRepository: overrides);

        var result = await service.GetByIdAsync(session.Id, actorId, TestContext.Current.CancellationToken);

        Assert.Equal(AccessLevel.Inject, result.EffectiveAccess);
    }

    [Fact]
    public async Task SessionDetail_Hides_Private_Session_From_NonOwner()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        var service = CreateSessionReader(session);

        await Assert.ThrowsAsync<NotFoundException>(() =>
            service.GetByIdAsync(session.Id, actorId, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task AuthorizedUsers_Include_Explicitly_Granted_User_Outside_Scope()
    {
        var ownerId = UserId.New();
        var friendId = UserId.New();
        var outsiderId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var friendships = new FakeFriendshipRepository(friendIds: [friendId]);
        var overrides = new FakeAccessOverrideRepository(
            SessionAccessOverride.Create(session.Id, outsiderId, AccessLevel.Inject, ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow));
        var accessService = CreateAccessService(
            friendshipRepository: friendships,
            accessOverrideRepository: overrides);

        var authorizedUserIds = await accessService.GetAuthorizedUserIdsAsync(session, TestContext.Current.CancellationToken);

        Assert.Contains(ownerId, authorizedUserIds);
        Assert.Contains(friendId, authorizedUserIds);
        Assert.Contains(outsiderId, authorizedUserIds);
    }

    [Fact]
    public async Task AuthorizedUsers_Exclude_Expired_Guest_Override()
    {
        var now = DateTimeOffset.UtcNow;
        var ownerId = UserId.New();
        var guestId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        var accessOverride = SessionAccessOverride.Create(
            session.Id,
            guestId,
            AccessLevel.View,
            ownerId,
            now.AddHours(1),
            now);
        var clock = new TestTimeProvider(now.AddHours(2));
        var accessService = new SessionAccessService(
            new FakeFriendshipRepository(),
            new FakeRoomMemberRepository(),
            new FakeAccessOverrideRepository(accessOverride),
            new FakeSessionViewerDismissalRepository(),
            clock);

        var authorized = await accessService.GetAuthorizedUserIdsAsync(
            session,
            TestContext.Current.CancellationToken);

        Assert.DoesNotContain(guestId, authorized);
        Assert.Contains(ownerId, authorized);
    }

    [Fact]
    public async Task Authorized_And_Denied_Batches_Apply_Dismissals_Without_PerViewer_Queries()
    {
        var ownerId = UserId.New();
        var dismissedViewerId = UserId.New();
        var otherViewerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var dismissals = new FakeSessionViewerDismissalRepository(
            SessionViewerDismissal.Create(
                session.Id,
                dismissedViewerId));
        var access = new SessionAccessService(
            new FakeFriendshipRepository(
                friendIds: [dismissedViewerId, otherViewerId]),
            new FakeRoomMemberRepository(),
            new FakeAccessOverrideRepository(),
            dismissals);

        var authorized = await access.GetAuthorizedUserIdsAsync(
            session,
            TestContext.Current.CancellationToken);
        var denied = await access.GetDeniedUserIdsAsync(
            session,
            [dismissedViewerId, otherViewerId],
            TestContext.Current.CancellationToken);

        Assert.Contains(ownerId, authorized);
        Assert.Contains(otherViewerId, authorized);
        Assert.DoesNotContain(dismissedViewerId, authorized);
        Assert.Contains(dismissedViewerId, denied);
        Assert.DoesNotContain(otherViewerId, denied);
        Assert.Equal(2, dismissals.DismissedViewerBatchCalls);
        Assert.Equal(0, dismissals.ExistsCalls);
    }

}
