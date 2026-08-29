using System.Text;
using System.Text.Json;
using System.Net.WebSockets;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;

using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

internal sealed class SessionAccessRefreshService
{
    public SessionAccessOverrideRevoker Revoker { get; }
    public SessionAccessDisconnector Disconnector { get; }

    public SessionAccessRefreshService(
        ISessionRepository sessionRepository,
        IAccessOverrideRepository overrides,
        IUnitOfWork unitOfWork,
        SessionReader sessionReader,
        IConnectionRegistry connectionRegistry,
        ILiveSessionStateDirectory runtimes,
        SessionBroadcaster broadcaster,
        OperationalMetrics metrics,
        ILogger<SessionAccessRefreshService> logger)
    {
        _ = unitOfWork;
        Revoker = new SessionAccessOverrideRevoker(
            sessionRepository,
            overrides,
            new FakeSessionKeyBlobRepository());
        Disconnector = new SessionAccessDisconnector(
            sessionReader,
            sessionRepository,
            connectionRegistry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            new TypedLoggerAdapter<SessionAccessDisconnector>(logger));
    }

    public async Task<IReadOnlyList<RoomLiveSession>> RevokeRoomMemberOverridesAsync(
        RoomId roomId, UserId removedUserId, CancellationToken ct = default)
    {
        var sessionIds = await Revoker.GetNonEndedRoomSessionIdsAsync(roomId, ct);
        return await Revoker.RevokeRoomMemberOverridesAsync(
            roomId,
            removedUserId,
            sessionIds,
            ct);
    }

    public Task DisconnectRemovedRoomMemberAsync(
        UserId removedUserId, IReadOnlyList<RoomLiveSession> sessions, CancellationToken ct = default)
        => Disconnector.DisconnectRemovedRoomMemberAsync(removedUserId, sessions, ct);

    public Task<IReadOnlyList<SessionAccessFanoutTarget>> RevokeFriendshipOverridesAsync(
        UserId ownerId, UserId formerFriendId, CancellationToken ct = default)
        => Revoker.RevokeFriendshipOverridesAsync(ownerId, formerFriendId, ct);

    public Task DisconnectFormerFriendAsync(
        UserId ownerId, UserId formerFriendId, IReadOnlyList<SessionAccessFanoutTarget> sessions, CancellationToken ct = default)
        => Disconnector.DisconnectFormerFriendAsync(ownerId, formerFriendId, sessions, ct);

    private sealed class TypedLoggerAdapter<T>(ILogger inner) : ILogger<T>
    {
        private readonly ILogger _inner = inner;

        public IDisposable? BeginScope<TState>(TState state) where TState : notnull
            => _inner.BeginScope(state);

        public bool IsEnabled(LogLevel logLevel) => _inner.IsEnabled(logLevel);

        public void Log<TState>(
            LogLevel logLevel, EventId eventId, TState state, Exception? exception,
            Func<TState, Exception?, string> formatter)
            => _inner.Log(logLevel, eventId, state, exception, formatter);
    }
}

public sealed class SessionAccessRefreshTests
{
    [Fact]
    public async Task ScopeChange_Disconnect_Ignores_AlreadyCancelled_Request_Token()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "room-session",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            roomId);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var sessionRepository = new RoomRefreshSessionRepository(session);
        var reader = new SessionReader(
            sessionRepository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository()),
            new FakeRuntimeDirectory());
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant("viewer", viewerId, "device", session.Id);
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionIncarnationId(session.IncarnationId);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var queue = broadcaster.GetOrCreateSession(session.Id)
            .AddParticipantQueue("viewer");
        var disconnector = new SessionAccessDisconnector(
            reader,
            sessionRepository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);
        using var cancelled = new CancellationTokenSource();
        cancelled.Cancel();

        await disconnector.DisconnectAfterScopeChangeAsync(
            session.Id,
            ownerId,
            session.StartedAt,
            cancelled.Token);

        Assert.Equal(QueueCompletionCause.AccessRevokedCascade, queue.CompletionCause);
    }

    [Fact]
    public async Task ScopeChange_Db_Revalidation_Failure_Refreshes_All_Shared_Participants()
    {
        var ownerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "scope-db-failure",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var repository = new ToggleFailureSessionRepository(session)
        {
            ThrowOnRead = true,
        };
        var runtimes = new LiveSessionStateDirectory();
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionIncarnationId(session.IncarnationId);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var registry = new ConnectionRegistry();
        var firstViewer = UserId.New();
        var secondViewer = UserId.New();
        registry.RegisterSharedParticipant(
            "first",
            firstViewer,
            "first-device",
            session.Id);
        registry.RegisterSharedParticipant(
            "second",
            secondViewer,
            "second-device",
            session.Id);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var firstQueue = queues.AddParticipantQueue(
            "first",
            AccessLevel.Inject);
        var secondQueue = queues.AddParticipantQueue(
            "second",
            AccessLevel.Inject);
        var reader = new SessionReader(
            repository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository()),
            runtimes);
        var disconnector = new SessionAccessDisconnector(
            reader,
            repository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectAfterScopeChangeAsync(
            session.Id,
            ownerId,
            session.StartedAt,
            TestContext.Current.CancellationToken);

        Assert.Equal(QueueCompletionCause.AccessRefresh, firstQueue.CompletionCause);
        Assert.Equal(QueueCompletionCause.AccessRefresh, secondQueue.CompletionCause);
    }

    [Fact]
    public async Task ScopeChange_Access_Lookup_Failure_Refreshes_All_Shared_Participants()
    {
        var ownerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "scope-access-failure",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var repository = new RoomRefreshSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionIncarnationId(session.IncarnationId);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant(
            "first",
            UserId.New(),
            "first-device",
            session.Id);
        registry.RegisterSharedParticipant(
            "second",
            UserId.New(),
            "second-device",
            session.Id);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var firstQueue = queues.AddParticipantQueue(
            "first",
            AccessLevel.Inject);
        var secondQueue = queues.AddParticipantQueue(
            "second",
            AccessLevel.Inject);
        var reader = new SessionReader(
            repository,
            new SessionAccessService(
                new ThrowingFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository()),
            runtimes);
        var disconnector = new SessionAccessDisconnector(
            reader,
            repository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectAfterScopeChangeAsync(
            session.Id,
            ownerId,
            session.StartedAt,
            TestContext.Current.CancellationToken);

        Assert.Equal(QueueCompletionCause.AccessRefresh, firstQueue.CompletionCause);
        Assert.Equal(QueueCompletionCause.AccessRefresh, secondQueue.CompletionCause);
    }

    [Fact]
    public async Task Delayed_ScopeChange_Cleanup_Does_Not_Touch_Republished_Session()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "old-scope-incarnation",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var oldStartedAt = session.StartedAt;
        session.End();
        await Task.Delay(1, TestContext.Current.CancellationToken);
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "replacement-scope-incarnation",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "new-secret",
            roomId: null);
        var repository = new RoomRefreshSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        var runtime = runtimes.CreateRuntime(session.Id);
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant(
            "replacement-viewer",
            viewerId,
            "replacement-device",
            session.Id);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionIncarnationId(session.IncarnationId);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var replacementQueue =
            queues.AddParticipantQueue("replacement-viewer");
        var reader = new SessionReader(
            repository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository()),
            runtimes);
        var disconnector = new SessionAccessDisconnector(
            reader,
            repository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectAfterScopeChangeAsync(
            session.Id,
            ownerId,
            oldStartedAt,
            TestContext.Current.CancellationToken);

        Assert.Null(replacementQueue.CompletionCause);
        Assert.Same(queues, broadcaster.TryGetSession(session.Id));
    }

    [Fact]
    public async Task Membership_Removal_Revokes_Override_On_Pending_Session()
    {
        var ownerId = UserId.New();
        var removedUserId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "pending-room-session",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            roomId);
        var accessOverride = SessionAccessOverride.Create(
            session.Id,
            removedUserId,
            AccessLevel.Inject,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(accessOverride);
        var revoker = new SessionAccessOverrideRevoker(
            new RoomRefreshSessionRepository(session),
            overrides,
            new FakeSessionKeyBlobRepository());

        var affected = await revoker.RevokeRoomMemberOverridesAsync(
            roomId,
            removedUserId,
            await revoker.GetNonEndedRoomSessionIdsAsync(
                roomId,
                TestContext.Current.CancellationToken),
            TestContext.Current.CancellationToken);

        Assert.Single(affected);
        Assert.False(accessOverride.IsActiveAt(DateTimeOffset.UtcNow));
    }

    [Fact]
    public async Task Membership_Removal_Ends_Removed_Owners_And_Fences_Surviving_Owners()
    {
        var removedUserId = UserId.New();
        var survivingOwnerId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var removedOwnerSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            removedUserId, "removed-owner", SessionScope.Room, ToolKind.Terminal,
            AccessLevel.View, "secret", roomId);
        var survivingSession = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            survivingOwnerId, "survivor", SessionScope.Room, ToolKind.Terminal,
            AccessLevel.View, "secret", roomId);
        var revoker = new SessionAccessOverrideRevoker(
            new RoomRefreshSessionRepository(removedOwnerSession, survivingSession),
            new FakeAccessOverrideRepository(),
            new FakeSessionKeyBlobRepository());

        var affected = await revoker.RevokeRoomMemberOverridesAsync(
            roomId,
            removedUserId,
            await revoker.GetNonEndedRoomSessionIdsAsync(
                roomId,
                TestContext.Current.CancellationToken),
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionStatus.Ended, removedOwnerSession.Status);
        Assert.Equal(0, removedOwnerSession.CurrentKeyGeneration);
        Assert.Equal(SessionStatus.Pending, survivingSession.Status);
        Assert.Equal(1, survivingSession.CurrentKeyGeneration);
        Assert.Contains(affected, item =>
            item.SessionId == removedOwnerSession.Id && item.EndedByRemoval);
        Assert.Contains(affected, item =>
            item.SessionId == survivingSession.Id && !item.EndedByRemoval);
    }

    [Fact]
    public async Task Membership_Removal_Does_Not_Cap_Override_Cascade()
    {
        var ownerId = UserId.New();
        var removedUserId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var sessions = Enumerable.Range(0, 501)
            .Select(index => Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                ownerId,
                $"room-session-{index}",
                SessionScope.Room,
                ToolKind.Terminal,
                AccessLevel.View,
                "secret",
                roomId))
            .ToArray();
        var overrides = sessions
            .Select(session => SessionAccessOverride.Create(
                session.Id,
                removedUserId,
                AccessLevel.Inject,
                ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow))
            .ToArray();
        var repository = new FakeAccessOverrideRepository(overrides);
        var revoker = new SessionAccessOverrideRevoker(
            new RoomRefreshSessionRepository(sessions),
            repository,
            new FakeSessionKeyBlobRepository());

        var affected = await revoker.RevokeRoomMemberOverridesAsync(
            roomId,
            removedUserId,
            await revoker.GetNonEndedRoomSessionIdsAsync(
                roomId,
                TestContext.Current.CancellationToken),
            TestContext.Current.CancellationToken);

        Assert.Equal(501, affected.Count);
        Assert.All(overrides, accessOverride => Assert.False(accessOverride.IsActiveAt(DateTimeOffset.UtcNow)));
    }

    [Fact]
    public async Task Delayed_RoomRemoval_Disconnect_Does_Not_Touch_Republished_Session()
    {
        var ownerId = UserId.New();
        var removedUserId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "old-room-incarnation",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            roomId);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var oldStartedAt = session.StartedAt;
        session.End();
        await Task.Delay(1, TestContext.Current.CancellationToken);
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "replacement",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "new-secret",
            roomId: null);
        var repository = new RoomRefreshSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        var runtime = runtimes.CreateRuntime(session.Id);
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var replacementQueues = broadcaster.GetOrCreateSession(session.Id);
        var replacementHostQueue =
            replacementQueues.SetHostQueue("replacement-host");
        var reader = new SessionReader(
            repository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository()),
            runtimes);
        var disconnector = new SessionAccessDisconnector(
            reader,
            repository,
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectRemovedRoomMemberAsync(
            removedUserId,
            [
                new RoomLiveSession(
                    session.Id,
                    ownerId,
                    oldStartedAt),
            ],
            TestContext.Current.CancellationToken);

        Assert.Same(replacementQueues, broadcaster.TryGetSession(session.Id));
        Assert.Null(replacementHostQueue.CompletionReason);
    }

    [Fact]
    public async Task RoomRemoval_Fails_One_Session_Closed_And_Continues_Remaining()
    {
        var removedUserId = UserId.New();
        var ownerId = UserId.New();
        var first = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "first-room-session",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        var second = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "second-room-session",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        first.ActivateHost("test-host");
        first.ReleaseHostSlot("test-host");
        second.ActivateHost("test-host");
        second.ReleaseHostSlot("test-host");
        var repository = new SelectiveFailureSessionRepository(first, second);
        var runtimes = new LiveSessionStateDirectory();
        var firstRuntime = runtimes.CreateRuntime(first.Id);
        firstRuntime.Host.SetSessionIncarnationId(first.IncarnationId);
        firstRuntime.Host.SetSessionStartedAt(first.StartedAt);
        var secondRuntime = runtimes.CreateRuntime(second.Id);
        secondRuntime.Host.SetSessionIncarnationId(second.IncarnationId);
        secondRuntime.Host.SetSessionStartedAt(second.StartedAt);
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant(
            "first-viewer",
            removedUserId,
            "device-1",
            first.Id);
        registry.RegisterSharedParticipant(
            "second-viewer",
            removedUserId,
            "device-2",
            second.Id);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var firstQueues = broadcaster.GetOrCreateSession(first.Id);
        var firstQueue =
            firstQueues.AddParticipantQueue("first-viewer");
        var secondQueue = broadcaster.GetOrCreateSession(second.Id)
            .AddParticipantQueue("second-viewer");
        var reader = new SessionReader(
            repository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository()),
            runtimes);
        var disconnector = new SessionAccessDisconnector(
            reader,
            repository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectRemovedRoomMemberAsync(
            removedUserId,
            [
                new RoomLiveSession(
                    first.Id,
                    ownerId,
                    first.StartedAt,
                    first.IncarnationId),
                new RoomLiveSession(
                    second.Id,
                    ownerId,
                    second.StartedAt,
                    second.IncarnationId),
            ],
            TestContext.Current.CancellationToken);

        Assert.Same(firstRuntime, runtimes.TryGet(first.Id));
        Assert.Same(firstQueues, broadcaster.TryGetSession(first.Id));
        Assert.Equal(
            QueueCompletionCause.AccessRefresh,
            firstQueue.CompletionCause);
        Assert.Equal(
            QueueCompletionCause.AccessRevokedCascade,
            secondQueue.CompletionCause);
    }

    [Fact]
    public async Task Failed_Room_Fanout_Does_Not_Touch_Mismatched_Runtime_Incarnation()
    {
        var ownerId = UserId.New();
        var removedUserId = UserId.New();
        var session = Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            1,
            Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "room-fanout-incarnation-fence",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            RoomId.From(Guid.NewGuid()));
        session.ActivateHost("host");
        session.ReleaseHostSlot("host");
        var repository = new FanoutFailureCountRepository(session, initialCount: 0);
        var runtimes = new LiveSessionStateDirectory();
        var runtime = runtimes.CreateRuntime(session.Id);
        runtime.Host.SetSessionIncarnationId(Guid.CreateVersion7());
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var hostQueue = queues.SetHostQueue();
        var participantQueue = queues.AddParticipantQueue("viewer");
        var disconnector = new SessionAccessDisconnector(
            new SessionReader(
                repository,
                new SessionAccessService(
                    new EmptyFriendshipRepository(),
                    new FakeRoomMemberRepository(),
                    new FakeAccessOverrideRepository(),
                    new FakeSessionViewerDismissalRepository()),
                runtimes),
            repository,
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectRemovedRoomMemberAsync(
            removedUserId,
            [new RoomLiveSession(
                session.Id,
                ownerId,
                session.StartedAt,
                session.IncarnationId)],
            TestContext.Current.CancellationToken);

        Assert.Null(hostQueue.CompletionReason);
        Assert.Null(participantQueue.CompletionCause);
    }

    [Fact]
    public async Task Failed_Room_Fanout_Preserves_Counted_Runtime_Until_Teardown_Then_Republishes_At_Capacity()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "counted-fanout-failure",
            SessionScope.Room,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret",
            RoomId.From(Guid.NewGuid()));
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var repository = new FanoutFailureCountRepository(
            session,
            initialCount: 1);
        var runtimes = new LiveSessionStateDirectory();
        var runtime = runtimes.CreateRuntime(session.Id);
        runtime.Host.SetSessionIncarnationId(session.IncarnationId);
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        runtime.Participants.TryAddSharedParticipant(
            "viewer",
            50,
            out _);
        var connections = new ConnectionRegistry();
        connections.RegisterSharedParticipant(
            "viewer",
            viewerId,
            "viewer-device",
            session.Id);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var participantQueue =
            queues.AddParticipantQueue("viewer");
        var lifecycleGate = new SessionLifecycleGate();
        var reader = new SessionReader(
            repository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository()),
            runtimes);
        var disconnector = new SessionAccessDisconnector(
            reader,
            repository,
            connections,
            runtimes,
            broadcaster,
            lifecycleGate,
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectRemovedRoomMemberAsync(
            viewerId,
            [new RoomLiveSession(
                session.Id,
                ownerId,
                session.StartedAt,
                session.IncarnationId)],
            TestContext.Current.CancellationToken);

        Assert.Same(runtime, runtimes.TryGet(session.Id));
        Assert.Same(runtime, runtimes.CreateRuntime(session.Id));
        Assert.Same(queues, broadcaster.TryGetSession(session.Id));
        Assert.Equal(
            QueueCompletionCause.AccessRefresh,
            participantQueue.CompletionCause);
        Assert.Equal(1, repository.Count);

        using var services = new ServiceCollection()
            .AddSingleton<ISessionRepository>(repository)
            .BuildServiceProvider();
        var state = new ParticipantConnectionState(
            "viewer",
            session.Id.Value.ToString(),
            viewerId)
        {
            SessionId = session.Id,
            Ports = runtime,
            SessionQueues = queues,
            ParticipantQueue = participantQueue,
            ParticipantRegistered = true,
            DbParticipantCounted = true,
            AccessDecision = new ParticipantAccessDecision(
                IsOwnerParticipant: false,
                AccessLevel.View,
                session.StartedAt),
        };
        var teardown = new ParticipantTeardown(
            connections,
            runtimes,
            services.GetRequiredService<IServiceScopeFactory>(),
            broadcaster,
            metrics,
            lifecycleGate,
            NullLogger<ParticipantTeardown>.Instance,
            new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new ClosedTestWebSocket(),
            state,
            TestContext.Current.CancellationToken);

        Assert.Equal(0, repository.Count);
        Assert.True(runtimes.RemoveIfSame(session.Id, runtime));
        Assert.True(broadcaster.RemoveSessionIfSame(
            session.Id,
            queues));
        var oldStartedAt = session.StartedAt;
        session.End();
        await Task.Delay(1, TestContext.Current.CancellationToken);
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "republished-after-fanout-failure",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "new-secret",
            roomId: null);
        Assert.NotEqual(oldStartedAt, session.StartedAt);
        var replacement = runtimes.CreateRuntime(session.Id);
        replacement.Host.SetSessionStartedAt(session.StartedAt);

        Assert.True(await repository.TryIncrementParticipantCountAsync(
            session.Id,
            session.StartedAt,
            maxParticipants: 1,
            TestContext.Current.CancellationToken));
        Assert.Equal(1, repository.Count);
    }

    [Fact]
    public async Task RefreshRoomParticipantAccessAfterRemovalAsync_Disconnects_Removed_Participant()
    {
        var ownerUserId = UserId.New();
        var removedUserId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId,
            "room-session",
            SessionScope.Room,
            ToolKind.ClaudeCode,
            AccessLevel.Suggest,
            "secret",
            roomId);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");

        var sessionRepository = new RoomRefreshSessionRepository(session);
        var accessService = new SessionAccessService(
            new EmptyFriendshipRepository(),
            new FakeRoomMemberRepository(),
            new FakeAccessOverrideRepository(),
            new FakeSessionViewerDismissalRepository());
        var sessionService = new SessionReader(sessionRepository, accessService, new FakeRuntimeDirectory());
        var connectionRegistry = new ConnectionRegistry();
        connectionRegistry.RegisterSharedParticipant("viewer-1", removedUserId, "viewer-device", session.Id);
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        runtimeDirectory.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var ownerQueue = queues.SetHostQueue();
        var viewerQueue = queues.AddParticipantQueue("viewer-1");
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            viewerQueue.TryEnqueueFrame(WireMessage.Json(Encoding.UTF8.GetBytes("stale-frame"))));

        var refreshService = new SessionAccessRefreshService(
            sessionRepository,
            new FakeAccessOverrideRepository(),
            new FakeUnitOfWork(),
            sessionService,
            connectionRegistry,
            runtimeDirectory,
            broadcaster,
            metrics,
            NullLogger<SessionAccessRefreshService>.Instance);

        var affectedSessions = await refreshService.RevokeRoomMemberOverridesAsync(
            roomId,
            removedUserId,
            TestContext.Current.CancellationToken);
        await refreshService.DisconnectRemovedRoomMemberAsync(
            removedUserId,
            affectedSessions,
            TestContext.Current.CancellationToken);

        var revokedViewerMessage = await viewerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(revokedViewerMessage);
        var revoked = JsonSerializer.Deserialize(
            Encoding.UTF8.GetString(revokedViewerMessage.Value.Payload),
            WsJsonContext.Default.SessionAccessRevokedMessage);
        Assert.Equal(session.Id.Value.ToString(), revoked?.SessionId);
        Assert.Null(await viewerQueue.ReadAsync(CancellationToken.None));

        var ownerMessage = await ownerQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(ownerMessage);
        var ownerRevoked = JsonSerializer.Deserialize(
            ownerMessage!,
            WsJsonContext.Default.HostAccessRevokedMessage);
        Assert.Equal(session.Id.Value.ToString(), ownerRevoked?.SessionId);
        Assert.Equal(removedUserId.Value.ToString(), ownerRevoked?.RevokedUserId);
    }

    [Fact]
    public async Task RefreshAfterMemberRemovalAsync_Revokes_Explicit_Override_From_Removed_Member()
    {
        var ownerUserId = UserId.New();
        var removedUserId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerUserId,
            "room-session",
            SessionScope.Room,
            ToolKind.ClaudeCode,
            AccessLevel.View,
            "secret",
            roomId);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var activeOverride = SessionAccessOverride.Create(
            session.Id,
            removedUserId,
            AccessLevel.Inject,
            ownerUserId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);

        var sessionRepository = new RoomRefreshSessionRepository(session);
        var sharedOverrides = new FakeAccessOverrideRepository(activeOverride);
        var accessService = new SessionAccessService(
            new EmptyFriendshipRepository(),
            new FakeRoomMemberRepository(),
            sharedOverrides,
            new FakeSessionViewerDismissalRepository());
        var sessionService = new SessionReader(sessionRepository, accessService, new FakeRuntimeDirectory());
        var connectionRegistry = new ConnectionRegistry();
        connectionRegistry.RegisterSharedParticipant("viewer-1", removedUserId, "viewer-device", session.Id);
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        runtimeDirectory.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        queues.SetHostQueue();
        var viewerQueue = queues.AddParticipantQueue("viewer-1");

        var refreshService = new SessionAccessRefreshService(
            sessionRepository,
            sharedOverrides,
            new FakeUnitOfWork(),
            sessionService,
            connectionRegistry,
            runtimeDirectory,
            broadcaster,
            metrics,
            NullLogger<SessionAccessRefreshService>.Instance);

        var affectedSessions = await refreshService.RevokeRoomMemberOverridesAsync(
            roomId,
            removedUserId,
            TestContext.Current.CancellationToken);
        await refreshService.DisconnectRemovedRoomMemberAsync(
            removedUserId,
            affectedSessions,
            TestContext.Current.CancellationToken);

        Assert.False(activeOverride.IsActiveAt(DateTimeOffset.UtcNow));
        Assert.Equal(QueueCompletionCause.AccessRevokedCascade, viewerQueue.CompletionCause);
    }

    [Fact]
    public async Task ExpiredOverride_Disconnects_Viewer_And_Notifies_Host_For_Key_Rotation()
    {
        var ownerId = UserId.New();
        var guestId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "guest-session",
            SessionScope.JustMe,
            ToolKind.ClaudeCode,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var sessionRepository = new RoomRefreshSessionRepository(session);
        var sessionReader = new SessionReader(
            sessionRepository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository()),
            new FakeRuntimeDirectory());
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant(
            "guest-connection",
            guestId,
            "guest-device",
            session.Id);
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var runtime = runtimes.CreateRuntime(session.Id);
        runtime.Host.SetSessionIncarnationId(session.IncarnationId);
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var hostQueue = queues.SetHostQueue();
        var guestQueue = queues.AddParticipantQueue("guest-connection");
        var disconnector = new SessionAccessDisconnector(
            sessionReader,
            sessionRepository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectExpiredOverrideAsync(
            new SessionAccessFanoutTarget(
                session.Id,
                session.StartedAt,
                session.IncarnationId),
            guestId,
            TestContext.Current.CancellationToken);

        Assert.Equal(
            QueueCompletionCause.AccessRevokedCascade,
            guestQueue.CompletionCause);
        var hostMessage = await hostQueue.ReadAsync(TestContext.Current.CancellationToken);
        var revoked = JsonSerializer.Deserialize(
            hostMessage!,
            WsJsonContext.Default.HostAccessRevokedMessage);
        Assert.Equal(guestId.Value.ToString(), revoked?.RevokedUserId);
    }

    [Fact]
    public async Task Delayed_ExpiredOverride_Does_Not_Mutate_Mismatched_Runtime()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            1,
            Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "expired-incarnation-fence",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var repository = new RoomRefreshSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        var runtime = runtimes.CreateRuntime(session.Id);
        runtime.Host.SetSessionIncarnationId(Guid.CreateVersion7());
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant(
            "viewer",
            viewerId,
            "viewer-device",
            session.Id);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queue = broadcaster.GetOrCreateSession(session.Id)
            .AddParticipantQueue("viewer");
        var disconnector = new SessionAccessDisconnector(
            new SessionReader(
                repository,
                new SessionAccessService(
                    new EmptyFriendshipRepository(),
                    new FakeRoomMemberRepository(),
                    new FakeAccessOverrideRepository(),
                    new FakeSessionViewerDismissalRepository()),
                runtimes),
            repository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectExpiredOverrideAsync(
            new SessionAccessFanoutTarget(
                session.Id,
                session.StartedAt,
                session.IncarnationId),
            viewerId,
            TestContext.Current.CancellationToken);

        Assert.Null(queue.CompletionCause);
    }

    [Fact]
    public async Task Delayed_Revoke_Does_Not_Disconnect_Newly_Regranted_Access()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "regranted-session",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var activeOverride = SessionAccessOverride.Create(
            session.Id,
            viewerId,
            AccessLevel.View,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(activeOverride);
        var repository = new RoomRefreshSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionIncarnationId(session.IncarnationId);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant(
            "viewer",
            viewerId,
            "device",
            session.Id);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var viewerQueue = queues.AddParticipantQueue("viewer");
        var reader = new SessionReader(
            repository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                overrides,
                new FakeSessionViewerDismissalRepository()),
            runtimes);
        var disconnector = new SessionAccessDisconnector(
            reader,
            repository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectRevokedAccessAsync(
            new SharingMutationResult(
                session.Id,
                session.IncarnationId,
                session.StartedAt,
                ownerId,
                viewerId,
                Granted: false),
            TestContext.Current.CancellationToken);

        Assert.Null(viewerQueue.CompletionCause);
    }

    [Fact]
    public async Task Revoke_Downgrade_Forces_Reconnect_With_Recomputed_Access()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "downgraded-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var accessOverride = SessionAccessOverride.Create(
            session.Id,
            viewerId,
            AccessLevel.Inject,
            ownerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        accessOverride.Revoke();
        var repository = new RoomRefreshSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionIncarnationId(session.IncarnationId);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant(
            "viewer",
            viewerId,
            "device",
            session.Id);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queue = broadcaster.GetOrCreateSession(session.Id)
            .AddParticipantQueue("viewer", AccessLevel.Inject);
        var reader = new SessionReader(
            repository,
            new SessionAccessService(
                new FakeFriendshipRepository(
                    friendIds: [viewerId]),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(accessOverride),
                new FakeSessionViewerDismissalRepository()),
            runtimes);
        var disconnector = new SessionAccessDisconnector(
            reader,
            repository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectRevokedAccessAsync(
            new SharingMutationResult(
                session.Id,
                session.IncarnationId,
                session.StartedAt,
                ownerId,
                viewerId,
                Granted: false),
            TestContext.Current.CancellationToken);

        Assert.Equal(QueueCompletionCause.AccessRefresh, queue.CompletionCause);
    }

    [Fact]
    public async Task Revocation_Revalidation_Failure_Disconnects_FailClosed_And_Recovers()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "fail-closed-session",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var repository = new ToggleFailureSessionRepository(session)
        {
            ThrowOnRead = true,
        };
        var runtimes = new LiveSessionStateDirectory();
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionIncarnationId(session.IncarnationId);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant(
            "first",
            viewerId,
            "device-1",
            session.Id);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var firstQueue = queues.AddParticipantQueue("first");
        var reader = new SessionReader(
            repository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository()),
            runtimes);
        var disconnector = new SessionAccessDisconnector(
            reader,
            repository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);
        var target = new SessionAccessFanoutTarget(
            session.Id,
            session.StartedAt,
            session.IncarnationId);

        await disconnector.DisconnectExpiredOverrideAsync(
            target,
            viewerId,
            TestContext.Current.CancellationToken);
        Assert.Equal(
            QueueCompletionCause.AccessRefresh,
            firstQueue.CompletionCause);

        repository.ThrowOnRead = false;
        registry.RegisterSharedParticipant(
            "second",
            viewerId,
            "device-2",
            session.Id);
        var secondQueue = queues.AddParticipantQueue("second");
        await disconnector.DisconnectExpiredOverrideAsync(
            target,
            viewerId,
            TestContext.Current.CancellationToken);
        Assert.Equal(
            QueueCompletionCause.AccessRevokedCascade,
            secondQueue.CompletionCause);
    }

    [Fact]
    public async Task Revocation_Fanout_Holds_Lifecycle_Lease_Through_Queue_Mutation()
    {
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "lease-held-session",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var repository = new BlockingReadSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionIncarnationId(session.IncarnationId);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant(
            "viewer",
            viewerId,
            "device",
            session.Id);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queue = broadcaster.GetOrCreateSession(session.Id)
            .AddParticipantQueue("viewer");
        var gate = new SessionLifecycleGate();
        var reader = new SessionReader(
            repository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository()),
            runtimes);
        var disconnector = new SessionAccessDisconnector(
            reader,
            repository,
            registry,
            runtimes,
            broadcaster,
            gate,
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);
        var disconnecting = disconnector.DisconnectExpiredOverrideAsync(
            new SessionAccessFanoutTarget(
                session.Id,
                session.StartedAt,
                session.IncarnationId),
            viewerId,
            TestContext.Current.CancellationToken);
        await repository.ReadStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        var republishLease = gate.AcquireAsync(
            session.Id,
            TestContext.Current.CancellationToken).AsTask();
        await Task.Delay(25, TestContext.Current.CancellationToken);
        Assert.False(republishLease.IsCompleted);

        repository.Release();
        await disconnecting;
        await using var acquired = await republishLease;
        Assert.Equal(
            QueueCompletionCause.AccessRevokedCascade,
            queue.CompletionCause);
    }

    [Fact]
    public async Task ViewerDismissal_Disconnects_Only_That_Viewer_And_Notifies_Host()
    {
        var ownerId = UserId.New();
        var dismissedViewerId = UserId.New();
        var otherViewerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "shared-session",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var sessionRepository = new RoomRefreshSessionRepository(session);
        var dismissal = SessionViewerDismissal.Create(
            session.Id,
            dismissedViewerId);
        var sessionReader = new SessionReader(
            sessionRepository,
            new SessionAccessService(
                new EmptyFriendshipRepository(),
                new FakeRoomMemberRepository(),
                new FakeAccessOverrideRepository(),
                new FakeSessionViewerDismissalRepository(dismissal)),
            new FakeRuntimeDirectory());
        var registry = new ConnectionRegistry();
        registry.RegisterSharedParticipant(
            "dismissed-connection",
            dismissedViewerId,
            "dismissed-device",
            session.Id);
        registry.RegisterSharedParticipant(
            "other-connection",
            otherViewerId,
            "other-device",
            session.Id);
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionIncarnationId(session.IncarnationId);
        runtimes.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var hostQueue = queues.SetHostQueue();
        var dismissedQueue = queues.AddParticipantQueue("dismissed-connection");
        var otherQueue = queues.AddParticipantQueue("other-connection");
        var disconnector = new SessionAccessDisconnector(
            sessionReader,
            sessionRepository,
            registry,
            runtimes,
            broadcaster,
            new SessionLifecycleGate(),
            metrics,
            NullLogger<SessionAccessDisconnector>.Instance);

        await disconnector.DisconnectDismissedViewerAsync(
            session.Id,
            session.IncarnationId,
            dismissedViewerId,
            session.StartedAt,
            TestContext.Current.CancellationToken);

        Assert.Equal(
            QueueCompletionCause.AccessRevokedCascade,
            dismissedQueue.CompletionCause);
        Assert.Null(otherQueue.CompletionCause);
        var hostMessage = await hostQueue.ReadAsync(TestContext.Current.CancellationToken);
        var revoked = JsonSerializer.Deserialize(
            hostMessage!,
            WsJsonContext.Default.HostAccessRevokedMessage);
        Assert.Equal(dismissedViewerId.Value.ToString(), revoked?.RevokedUserId);
    }

    [Fact]
    public async Task RefreshAfterFriendshipChangeAsync_Revokes_Explicit_Override_From_Former_Friend()
    {
        var alice = UserId.New();
        var bob = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            alice,
            "friends-session",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var override_ = SessionAccessOverride.Create(
            session.Id,
            bob,
            AccessLevel.Inject,
            alice, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);

        var sessionRepository = new RoomRefreshSessionRepository(session);
        var sharedOverrides = new FakeAccessOverrideRepository(override_);
        var accessService = new SessionAccessService(
            new EmptyFriendshipRepository(),
            new FakeRoomMemberRepository(),
            sharedOverrides,
            new FakeSessionViewerDismissalRepository());
        var sessionService = new SessionReader(sessionRepository, accessService, new FakeRuntimeDirectory());
        var connectionRegistry = new ConnectionRegistry();
        connectionRegistry.RegisterSharedParticipant("bob-conn", bob, "bob-device", session.Id);
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        runtimeDirectory.CreateRuntime(session.Id)
            .Host.SetSessionIncarnationId(session.IncarnationId);
        runtimeDirectory.CreateRuntime(session.Id)
            .Host.SetSessionStartedAt(session.StartedAt);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        queues.SetHostQueue();
        var bobQueue = queues.AddParticipantQueue("bob-conn");

        var refreshService = new SessionAccessRefreshService(
            sessionRepository,
            sharedOverrides,
            new FakeUnitOfWork(),
            sessionService,
            connectionRegistry,
            runtimeDirectory,
            broadcaster,
            metrics,
            NullLogger<SessionAccessRefreshService>.Instance);

        var aliceSessions = await refreshService.RevokeFriendshipOverridesAsync(
            alice, bob, TestContext.Current.CancellationToken);
        var bobSessions = await refreshService.RevokeFriendshipOverridesAsync(
            bob, alice, TestContext.Current.CancellationToken);
        await refreshService.DisconnectFormerFriendAsync(
            alice, bob, aliceSessions, TestContext.Current.CancellationToken);
        await refreshService.DisconnectFormerFriendAsync(
            bob, alice, bobSessions, TestContext.Current.CancellationToken);

        Assert.False(override_.IsActiveAt(DateTimeOffset.UtcNow));
        Assert.Equal(QueueCompletionCause.AccessRevokedCascade, bobQueue.CompletionCause);
    }

    [Fact]
    public async Task RefreshAfterFriendshipChangeAsync_Disconnects_Former_Friend_Bidirectionally()
    {
        var userA = UserId.New();
        var userB = UserId.New();
        var sessionA = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            userA,
            "friends-session-a",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.View,
            "secret");
        sessionA.ActivateHost("test-host");
        sessionA.ReleaseHostSlot("test-host");
        var sessionB = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            userB,
            "friends-session-b",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.View,
            "secret");
        sessionB.ActivateHost("test-host");
        sessionB.ReleaseHostSlot("test-host");

        var sessionRepository = new RoomRefreshSessionRepository(sessionA, sessionB);
        var accessService = new SessionAccessService(
            new EmptyFriendshipRepository(),
            new FakeRoomMemberRepository(),
            new FakeAccessOverrideRepository(),
            new FakeSessionViewerDismissalRepository());
        var sessionService = new SessionReader(sessionRepository, accessService, new FakeRuntimeDirectory());
        var connectionRegistry = new ConnectionRegistry();
        connectionRegistry.RegisterSharedParticipant("a-viewer", userB, "user-b-device", sessionA.Id);
        connectionRegistry.RegisterSharedParticipant("b-viewer", userA, "user-a-device", sessionB.Id);
        var runtimeDirectory = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        runtimeDirectory.CreateRuntime(sessionA.Id)
            .Host.SetSessionIncarnationId(sessionA.IncarnationId);
        runtimeDirectory.CreateRuntime(sessionA.Id)
            .Host.SetSessionStartedAt(sessionA.StartedAt);
        runtimeDirectory.CreateRuntime(sessionB.Id)
            .Host.SetSessionIncarnationId(sessionB.IncarnationId);
        runtimeDirectory.CreateRuntime(sessionB.Id)
            .Host.SetSessionStartedAt(sessionB.StartedAt);
        var queuesA = broadcaster.GetOrCreateSession(sessionA.Id);
        queuesA.SetHostQueue();
        var viewerA = queuesA.AddParticipantQueue("a-viewer");
        var queuesB = broadcaster.GetOrCreateSession(sessionB.Id);
        queuesB.SetHostQueue();
        var viewerB = queuesB.AddParticipantQueue("b-viewer");

        var refreshService = new SessionAccessRefreshService(
            sessionRepository,
            new FakeAccessOverrideRepository(),
            new FakeUnitOfWork(),
            sessionService,
            connectionRegistry,
            runtimeDirectory,
            broadcaster,
            metrics,
            NullLogger<SessionAccessRefreshService>.Instance);

        var aSessions = await refreshService.RevokeFriendshipOverridesAsync(
            userA, userB, TestContext.Current.CancellationToken);
        var bSessions = await refreshService.RevokeFriendshipOverridesAsync(
            userB, userA, TestContext.Current.CancellationToken);
        await refreshService.DisconnectFormerFriendAsync(
            userA, userB, aSessions, TestContext.Current.CancellationToken);
        await refreshService.DisconnectFormerFriendAsync(
            userB, userA, bSessions, TestContext.Current.CancellationToken);

        Assert.NotNull(viewerA.CompletionCause);
        Assert.NotNull(viewerB.CompletionCause);
        Assert.Equal(QueueCompletionCause.AccessRevokedCascade, viewerA.CompletionCause);
        Assert.Equal(QueueCompletionCause.AccessRevokedCascade, viewerB.CompletionCause);
    }

    private sealed class RoomRefreshSessionRepository(params Session[] sessions)
        : SessionRepositoryStub
    {
        private readonly IReadOnlyDictionary<SessionId, Session> _sessions = sessions.ToDictionary(session => session.Id);

        public override Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)
            => Task.FromResult(_sessions.TryGetValue(id, out var session) ? session : null);

        public override Task AddAsync(Session session, CancellationToken ct = default)
            => throw new NotSupportedException();

        public override Task UpdateAsync(Session session, CancellationToken ct = default)
            => Task.CompletedTask;

        public override Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(
            UserId ownerUserId,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<SessionCardProjection>>([]);

        public override Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
            RoomId roomId,
            FeedCursor? cursor = null,
            int limit = 20,
            ToolKind? toolKindFilter = null,
            DateTimeOffset? since = null,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<SessionCardProjection>>([]);

        public override Task<IReadOnlyList<SessionId>> GetAllNonEndedByRoomIdsAsync(
            RoomId roomId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SessionId>>(
                _sessions.Values
                    .Where(session =>
                        session.RoomId == roomId
                        && session.Scope == SessionScope.Room
                        && session.Status != SessionStatus.Ended)
                    .Select(session => session.Id)
                    .ToList());

        public override Task<IReadOnlyList<Session>> GetNonEndedByRoomIdsForUpdateAsync(
            RoomId roomId,
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<Session>>(
                _sessions.Values
                    .Where(session =>
                        sessionIds.Contains(session.Id)
                        && session.RoomId == roomId
                        && session.Scope == SessionScope.Room
                        && session.Status != SessionStatus.Ended)
                    .ToList());

        public override Task<IReadOnlyList<SessionCardProjection>> GetAllNonEndedFriendSessionsByOwnerAsync(
            UserId ownerUserId,
            CancellationToken ct = default)
        {
            IReadOnlyList<SessionCardProjection> projections = _sessions.Values
                .Where(session =>
                    session.OwnerUserId == ownerUserId
                    && session.Scope == SessionScope.Friends
                    && session.Status != SessionStatus.Ended)
                .Select(ToCard)
                .ToList();
            return Task.FromResult(projections);
        }

        private static SessionCardProjection ToCard(Session session) =>
            new(
                session.Id.Value,
                session.Title,
                session.Scope,
                session.DefaultAccess,
                session.Status,
                session.OwnerUserId.Value,
                "Owner",
                null,
                session.StartedAt,
                session.IncarnationId,
                session.ToolKind,
                session.RoomId?.Value);
    }

    private sealed class ToggleFailureSessionRepository(Session session)
        : SessionRepositoryStub
    {
        public bool ThrowOnRead { get; set; }

        public override Task<Session?> GetByIdAsync(
            SessionId id,
            CancellationToken ct = default)
        {
            if (ThrowOnRead)
            {
                throw new InvalidOperationException("Injected read failure.");
            }

            return Task.FromResult<Session?>(id == session.Id ? session : null);
        }
    }

    private sealed class ThrowingFriendshipRepository : IFriendshipRepository
    {
        private static InvalidOperationException Failure() =>
            new("Injected access lookup failure.");

        public Task<bool> AreFriendsAsync(
            UserId userA,
            UserId userB,
            CancellationToken ct = default) =>
            throw Failure();

        public Task<IReadOnlyList<UserId>> GetFriendIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw Failure();

        public Task<IReadOnlyList<Friendship>> GetPendingIncomingAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw Failure();

        public Task<IReadOnlyList<Friendship>> GetPendingOutgoingAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw Failure();

        public Task<Friendship?> GetAsync(
            UserId userA,
            UserId userB,
            CancellationToken ct = default) =>
            throw Failure();

        public Task AddAsync(
            Friendship friendship,
            CancellationToken ct = default) =>
            throw Failure();

        public void Remove(Friendship friendship) => throw Failure();
    }

    private sealed class SelectiveFailureSessionRepository(
        Session failing,
        Session succeeding) : SessionRepositoryStub
    {
        public override Task<Session?> GetByIdAsync(
            SessionId id,
            CancellationToken ct = default)
        {
            if (id == failing.Id)
            {
                throw new InvalidOperationException(
                    "Injected per-session revalidation failure.");
            }
            return Task.FromResult<Session?>(
                id == succeeding.Id ? succeeding : null);
        }
    }

    private sealed class FanoutFailureCountRepository(
        Session session,
        int initialCount) : SessionRepositoryStub
    {
        public int Count { get; private set; } = initialCount;

        public override Task<Session?> GetByIdAsync(
            SessionId id,
            CancellationToken ct = default) =>
            throw new InvalidOperationException(
                "Injected room fanout revalidation failure.");

        public override Task<bool> TryDecrementParticipantCountAsync(
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            CancellationToken ct = default)
        {
            if (session.Id != sessionId
                || session.StartedAt != expectedStartedAt
                || Count == 0)
            {
                return Task.FromResult(false);
            }

            Count--;
            return Task.FromResult(true);
        }

        public override Task<bool> TryIncrementParticipantCountAsync(
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            int maxParticipants,
            CancellationToken ct = default)
        {
            if (session.Id != sessionId
                || session.StartedAt != expectedStartedAt
                || Count >= maxParticipants)
            {
                return Task.FromResult(false);
            }

            Count++;
            return Task.FromResult(true);
        }
    }

    private sealed class BlockingReadSessionRepository(Session session)
        : SessionRepositoryStub
    {
        private readonly TaskCompletionSource _release =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource ReadStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public override async Task<Session?> GetByIdAsync(
            SessionId id,
            CancellationToken ct = default)
        {
            ReadStarted.TrySetResult();
            await _release.Task.WaitAsync(ct);
            return id == session.Id ? session : null;
        }

        public void Release() => _release.TrySetResult();
    }

    private sealed class ClosedTestWebSocket : WebSocket
    {
        public override WebSocketCloseStatus? CloseStatus =>
            WebSocketCloseStatus.NormalClosure;
        public override string? CloseStatusDescription => null;
        public override WebSocketState State => WebSocketState.Closed;
        public override string? SubProtocol => null;
        public override void Abort() { }
        public override Task CloseAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken) =>
            Task.CompletedTask;
        public override Task CloseOutputAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken) =>
            Task.CompletedTask;
        public override void Dispose() { }
        public override Task<WebSocketReceiveResult> ReceiveAsync(
            ArraySegment<byte> buffer,
            CancellationToken cancellationToken) =>
            throw new NotSupportedException();
        public override Task SendAsync(
            ArraySegment<byte> buffer,
            WebSocketMessageType messageType,
            bool endOfMessage,
            CancellationToken cancellationToken) =>
            throw new NotSupportedException();
    }

    private sealed class EmptyFriendshipRepository : IFriendshipRepository
    {
        public Task<bool> AreFriendsAsync(UserId userA, UserId userB, CancellationToken ct = default)
            => Task.FromResult(false);

        public Task<IReadOnlyList<UserId>> GetFriendIdsAsync(UserId userId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<IReadOnlyList<Friendship>> GetPendingIncomingAsync(
            UserId userId,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<Friendship>>([]);

        public Task<IReadOnlyList<Friendship>> GetPendingOutgoingAsync(
            UserId userId,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<Friendship>>([]);

        public Task<Friendship?> GetAsync(UserId userA, UserId userB, CancellationToken ct = default)
            => Task.FromResult<Friendship?>(null);

        public Task AddAsync(Friendship friendship, CancellationToken ct = default)
            => Task.CompletedTask;

        public void Remove(Friendship friendship)
        {
        }
    }

    private sealed class PassthroughOwnerSessionSecretHasher : IOwnerSessionSecretHasher
    {
        public string Hash(string secret) => secret;

        public bool Verify(string hashedSecret, string providedSecret) => hashedSecret == providedSecret;
    }
}
