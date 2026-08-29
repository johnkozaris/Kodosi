using Kodosi.Application;
using Kodosi.Domain;

using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class SessionViewerDismissalTests
{
    [Fact]
    public async Task Friend_Access_Is_Denied_After_Dismissal()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);

        await AssertAccessBeforeAndAfterDismissalAsync(
            session,
            viewerId,
            new FakeFriendshipRepository(areFriends: true),
            new FakeRoomMemberRepository(),
            new FakeAccessOverrideRepository());
    }

    [Fact]
    public async Task Room_Access_Is_Denied_After_Dismissal()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = CreateSession(ownerId, SessionScope.Room, roomId);

        await AssertAccessBeforeAndAfterDismissalAsync(
            session,
            viewerId,
            new FakeFriendshipRepository(),
            new FakeRoomMemberRepository((roomId, viewerId)),
            new FakeAccessOverrideRepository());
    }

    [Fact]
    public async Task Explicit_Override_Access_Is_Denied_After_Dismissal()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);

        await AssertAccessBeforeAndAfterDismissalAsync(
            session,
            viewerId,
            new FakeFriendshipRepository(),
            new FakeRoomMemberRepository(),
            new FakeAccessOverrideRepository(
                SessionAccessOverride.Create(
                    session.Id,
                    viewerId,
                    AccessLevel.Inject,
                    ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow)));
    }

    [Fact]
    public async Task Dismissal_Does_Not_Affect_Owner_Or_Different_Viewer()
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
            new FakeFriendshipRepository(friendIds: [dismissedViewerId, otherViewerId]),
            new FakeRoomMemberRepository(),
            new FakeAccessOverrideRepository(),
            dismissals);

        Assert.Equal(
            AccessLevel.Inject,
            await access.ResolveAccessAsync(
                session,
                ownerId,
                TestContext.Current.CancellationToken));
        Assert.Equal(
            session.DefaultAccess,
            await access.ResolveAccessAsync(
                session,
                otherViewerId,
                TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<PolicyViolationException>(() =>
            access.ResolveAccessAsync(
                session,
                dismissedViewerId,
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Dismiss_Is_Idempotent_And_Owner_SelfDismissal_Is_Hidden()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var sessions = new FakeSessionRepository(session);
        var dismissals = new FakeSessionViewerDismissalRepository();
        var access = new SessionAccessService(
            new FakeFriendshipRepository(areFriends: true),
            new FakeRoomMemberRepository(),
            new FakeAccessOverrideRepository(),
            dismissals);
        var mutations = new FakeSessionAccessMutationRepository();
        var service = new SessionViewerDismissalService(
            sessions,
            dismissals,
            access,
            mutations,
            new FakeUnitOfWork(),
            TimeProvider.System);
        var mutationId = Guid.CreateVersion7();

        await service.DismissAsync(
            session.Id,
            session.IncarnationId,
            mutationId,
            viewerId,
            TestContext.Current.CancellationToken);
        await service.DismissAsync(
            session.Id,
            session.IncarnationId,
            mutationId,
            viewerId,
            TestContext.Current.CancellationToken);

        Assert.Equal(1, dismissals.AddCalls);
        Assert.Equal(1, dismissals.Count);
        await Assert.ThrowsAsync<NotFoundException>(() =>
            service.DismissAsync(
                session.Id,
                session.IncarnationId,
                Guid.CreateVersion7(),
                ownerId,
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Dismiss_Exact_Retry_After_Republish_Does_Not_Project_Old_Incarnation()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var sessions = new FakeSessionRepository(session);
        var dismissals = new FakeSessionViewerDismissalRepository();
        var mutations = new FakeSessionAccessMutationRepository();
        var service = new SessionViewerDismissalService(
            sessions,
            dismissals,
            new SessionAccessService(
                new FakeFriendshipRepository(areFriends: true),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                dismissals),
            mutations,
            new FakeUnitOfWork(),
            TimeProvider.System);
        var mutationId = Guid.CreateVersion7();
        var oldIncarnationId = session.IncarnationId;

        var applied = await service.DismissAsync(
            session.Id,
            oldIncarnationId,
            mutationId,
            viewerId,
            TestContext.Current.CancellationToken);
        session.End();
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "replacement",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "replacement-secret",
            roomId: null);
        var replay = await service.DismissAsync(
            session.Id,
            oldIncarnationId,
            mutationId,
            viewerId,
            TestContext.Current.CancellationToken);

        Assert.True(applied.ShouldProject);
        Assert.False(replay.ShouldProject);
        Assert.Equal(oldIncarnationId, replay.IncarnationId);
        Assert.Equal(1, dismissals.AddCalls);
    }

    [Fact]
    public async Task Dismiss_Hides_Session_From_Viewer_Without_Current_Access()
    {
        var session = CreateSession(UserId.New(), SessionScope.JustMe);
        var viewerId = UserId.New();
        var sessions = new FakeSessionRepository(session);
        var dismissals = new FakeSessionViewerDismissalRepository();
        var access = new SessionAccessService(
            new FakeFriendshipRepository(),
            new FakeRoomMemberRepository(),
            new FakeAccessOverrideRepository(),
            dismissals);
        var service = new SessionViewerDismissalService(
            sessions,
            dismissals,
            access,
            new FakeSessionAccessMutationRepository(),
            new FakeUnitOfWork(),
            TimeProvider.System);

        await Assert.ThrowsAsync<NotFoundException>(() =>
            service.DismissAsync(
                session.Id,
                session.IncarnationId,
                Guid.CreateVersion7(),
                viewerId,
                TestContext.Current.CancellationToken));
        Assert.Equal(0, dismissals.Count);
    }

    [Fact]
    public async Task Dismiss_Rejects_Stale_Incarnation_After_Republish()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var staleIncarnationId = session.IncarnationId;
        session.End();
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "replacement",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "replacement-secret",
            roomId: null);
        var dismissals = new FakeSessionViewerDismissalRepository();
        var service = new SessionViewerDismissalService(
            new FakeSessionRepository(session),
            dismissals,
            new SessionAccessService(
                new FakeFriendshipRepository(areFriends: true),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                dismissals),
            new FakeSessionAccessMutationRepository(),
            new FakeUnitOfWork(),
            TimeProvider.System);

        await Assert.ThrowsAsync<NotFoundException>(() =>
            service.DismissAsync(
                session.Id,
                staleIncarnationId,
                Guid.CreateVersion7(),
                viewerId,
                TestContext.Current.CancellationToken));

        Assert.Equal(0, dismissals.Count);
    }

    [Fact]
    public async Task Discovery_Audience_Excludes_Dismissed_Viewer()
    {
        var ownerId = UserId.New();
        var dismissedViewerId = UserId.New();
        var otherViewerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.Friends);
        var resolver = new DiscoveryAudienceResolver(
            new FakeFriendshipRepository(
                friendIds: [dismissedViewerId, otherViewerId]),
            new FakeRoomMemberRepository(),
            new FakeSessionViewerDismissalRepository(
                SessionViewerDismissal.Create(
                    session.Id,
                    dismissedViewerId)));
        var target = new SessionDiscoveryTarget(
            session.Id,
            ownerId,
            session.Scope,
            session.RoomId);

        var audience = await resolver.ResolveSessionChangeAsync(
            target,
            target,
            TestContext.Current.CancellationToken);

        Assert.Contains(ownerId, audience.UserIds);
        Assert.Contains(otherViewerId, audience.UserIds);
        Assert.DoesNotContain(dismissedViewerId, audience.UserIds);
    }

    private static async Task AssertAccessBeforeAndAfterDismissalAsync(
        Session session,
        UserId viewerId,
        IFriendshipRepository friendships,
        IRoomMemberRepository roomMembers,
        IAccessOverrideRepository overrides)
    {
        var sessions = new FakeSessionRepository(session);
        var dismissals = new FakeSessionViewerDismissalRepository();
        var access = new SessionAccessService(
            friendships,
            roomMembers,
            overrides,
            dismissals);
        var reader = new SessionReader(
            sessions,
            access,
            new FakeRuntimeDirectory());
        var service = new SessionViewerDismissalService(
            sessions,
            dismissals,
            access,
            new FakeSessionAccessMutationRepository(),
            new FakeUnitOfWork(),
            TimeProvider.System);

        await reader.GetByIdAsync(
            session.Id,
            viewerId,
            TestContext.Current.CancellationToken);
        await service.DismissAsync(
            session.Id,
            session.IncarnationId,
            Guid.CreateVersion7(),
            viewerId,
            TestContext.Current.CancellationToken);

        await Assert.ThrowsAsync<NotFoundException>(() =>
            reader.GetByIdAsync(
                session.Id,
                viewerId,
                TestContext.Current.CancellationToken));
    }
}
