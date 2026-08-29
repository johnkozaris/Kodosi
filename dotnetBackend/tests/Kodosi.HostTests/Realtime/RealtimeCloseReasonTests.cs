using System.Net.WebSockets;
using System.Text;
using System.Text.Json;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Host.Serialization;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class RealtimeCloseReasonTests
{
    [Fact]
    public async Task ParticipantTeardown_Maps_Drain_To_ServerRestarting_Close()
    {
        var queue = new RelayClientSendQueue();
        queue.Complete(QueueCompletionCause.Drain, discardPending: true);
        var state = new ParticipantConnectionState("viewer-1", SessionId.New().Value.ToString(), UserId.New())
        {
            ParticipantQueue = queue,
        };
        var webSocket = new TestWebSocket();
        var teardown = CreateParticipantTeardown();

        await teardown.RunAsync(webSocket, state, TestContext.Current.CancellationToken);

        Assert.Equal(WebSocketCloseStatus.EndpointUnavailable, webSocket.LastCloseStatus);
        Assert.Equal(CloseReason.ServerRestarting.ToWire(), webSocket.LastCloseDescription);
    }

    [Fact]
    public async Task ParticipantTeardown_Preserves_Explicit_Ended_Close_Reason()
    {
        var queue = new RelayClientSendQueue();
        queue.Complete(QueueCompletionCause.SessionEnd, closeReason: CloseReason.HostStopped);
        var state = new ParticipantConnectionState("viewer-1", SessionId.New().Value.ToString(), UserId.New())
        {
            ParticipantQueue = queue,
        };
        var webSocket = new TestWebSocket();
        var teardown = CreateParticipantTeardown();

        await teardown.RunAsync(webSocket, state, TestContext.Current.CancellationToken);

        Assert.Equal(WebSocketCloseStatus.NormalClosure, webSocket.LastCloseStatus);
        Assert.Equal(CloseReason.HostStopped.ToWire(), webSocket.LastCloseDescription);
    }

    [Fact]
    public async Task ParticipantTeardown_Maps_Participant_Timeout_To_Retryable_Timeout_Close()
    {
        var queue = new RelayClientSendQueue();
        queue.Complete(QueueCompletionCause.Timeout, discardPending: true);
        var state = new ParticipantConnectionState("viewer-1", SessionId.New().Value.ToString(), UserId.New())
        {
            ParticipantQueue = queue,
        };
        var webSocket = new TestWebSocket();
        var teardown = CreateParticipantTeardown();

        await teardown.RunAsync(webSocket, state, TestContext.Current.CancellationToken);

        Assert.Equal(WebSocketCloseStatus.NormalClosure, webSocket.LastCloseStatus);
        Assert.Equal(CloseReason.ParticipantTimeout.ToWire(), webSocket.LastCloseDescription);
        Assert.Equal("timeout", webSocket.LastCloseDescription);
    }

    [Fact]
    public async Task ParticipantTeardown_Maps_Device_Expiry_To_AccessRevoked()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var state = new ParticipantConnectionState(
            "viewer-1",
            SessionId.New().Value.ToString(),
            UserId.New(),
            clock);
        state.StartDeviceAuthorizationLifetime(clock.GetUtcNow());
        var webSocket = new TestWebSocket();

        await CreateParticipantTeardown().RunAsync(
            webSocket,
            state,
            TestContext.Current.CancellationToken);

        Assert.Equal(WebSocketCloseStatus.PolicyViolation, webSocket.LastCloseStatus);
        Assert.Equal(CloseReason.AccessRevoked.ToWire(), webSocket.LastCloseDescription);
    }

    [Fact]
    public async Task ParticipantTeardown_Disposes_Persistence_Scope_After_Decrement()
    {
        var scopedLifetime = new ScopedLifetimeProbe();
        using var services = new ServiceCollection()
            .AddScoped(_ => scopedLifetime.CreateLease())
            .AddScoped<ISessionRepository>(serviceProvider =>
            {
                _ = serviceProvider.GetRequiredService<ScopedLifetimeLease>();
                return new FakeSessionRepository();
            })
            .BuildServiceProvider();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var ports = runtimes.CreateRuntime(sessionId);
        var startedAt = DateTimeOffset.UtcNow;
        ports.Host.SetSessionStartedAt(startedAt);
        var state = new ParticipantConnectionState(
            "viewer-1",
            sessionId.Value.ToString(),
            UserId.New())
        {
            SessionId = sessionId,
            Ports = ports,
            DbParticipantCounted = true,
            AccessDecision = new ParticipantAccessDecision(
                IsOwnerParticipant: false,
                AccessLevel.View,
                startedAt),
        };
        var teardown = new ParticipantTeardown(
            new ConnectionRegistry(),
            runtimes,
            services.GetRequiredService<IServiceScopeFactory>(),
            broadcaster,
            metrics,
            new SessionLifecycleGate(),
            NullLogger<ParticipantTeardown>.Instance,
            new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new TestWebSocket(),
            state,
            TestContext.Current.CancellationToken);

        Assert.False(state.DbParticipantCounted);
        Assert.Equal(1, scopedLifetime.Created);
        Assert.Equal(scopedLifetime.Created, scopedLifetime.Disposed);
    }

    [Fact]
    public void Session_Timeout_Uses_Dedicated_Terminal_Wire_Reason()
    {
        Assert.Equal("session_timeout", CloseReason.SessionTimeout.ToWire());
        Assert.Equal("timeout", CloseReason.ParticipantTimeout.ToWire());
    }

    [Fact]
    public void Session_Runtime_Recovering_Is_Retryable_Endpoint_Unavailable_Close()
    {
        Assert.Equal(
            "session_runtime_recovering",
            CloseReason.SessionRuntimeRecovering.ToWire());
        Assert.Equal(
            WebSocketCloseStatus.EndpointUnavailable,
            CloseReason.SessionRuntimeRecovering.ToWebSocketCloseStatus());
        Assert.Equal(
            WebSocketCloseStatus.PolicyViolation,
            CloseReason.SessionNotLive.ToWebSocketCloseStatus());
    }

    [Fact]
    public void Terminal_Replay_Gap_Uses_Dedicated_Recovery_Wire_Reason()
    {
        Assert.Equal("terminal_replay_gap", CloseReason.TerminalReplayGap.ToWire());
        Assert.Equal(
            WebSocketCloseStatus.PolicyViolation,
            CloseReason.TerminalReplayGap.ToWebSocketCloseStatus());
    }

    [Fact]
    public void Server_Error_Is_Retryable_Endpoint_Unavailable_Close()
    {
        Assert.Equal("server_error", CloseReason.ServerError.ToWire());
        Assert.Equal(WebSocketCloseStatus.EndpointUnavailable, CloseReason.ServerError.ToWebSocketCloseStatus());
    }

    [Fact]
    public async Task HostTeardown_Preserves_Forced_Host_Close_Reason()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var hostQueue = new ChannelByteSendQueue();
        hostQueue.Complete(CloseReason.OwnerIdentityReset);
        var state = new HostConnectionState("host-1", SessionId.New().Value.ToString(), UserId.New())
        {
            HostQueue = hostQueue,
        };
        var webSocket = new TestWebSocket();
        var teardown = new HostTeardown(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            new ServiceCollection().BuildServiceProvider().GetRequiredService<IServiceScopeFactory>(),
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(webSocket, state, TestContext.Current.CancellationToken);

        Assert.Equal(WebSocketCloseStatus.PolicyViolation, webSocket.LastCloseStatus);
        Assert.Equal(CloseReason.OwnerIdentityReset.ToWire(), webSocket.LastCloseDescription);
    }

    [Fact]
    public async Task HostTeardown_Removes_PreLive_Runtime_Before_Broadcaster_Queues()
    {
        var runtimes = new ObservingRuntimeDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var ports = runtimes.CreateRuntime(
            sessionId,
            out var runtimeCreationOwnership);
        using var hostLifetime = new CancellationTokenSource();
        Assert.True(ports.Host.TryClaimHost("host-1", hostLifetime));
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var hostQueue = queues.SetHostQueue("host-1");
        var broadcasterQueueStillPresentWhenRuntimeRemoved = false;
        runtimes.OnRemove = removedSessionId =>
        {
            if (removedSessionId == sessionId)
            {
                broadcasterQueueStillPresentWhenRuntimeRemoved = ReferenceEquals(
                    queues,
                    broadcaster.TryGetSession(sessionId));
            }
        };
        var state = new HostConnectionState("host-1", sessionId.Value.ToString(), UserId.New())
        {
            SessionId = sessionId,
            Ports = ports,
            SessionQueues = queues,
            HostQueue = hostQueue,
            RuntimeCreationOwnership = runtimeCreationOwnership,
            HostClaimed = true,
            DiscardPreAcceptedState = true,
        };
        var teardown = new HostTeardown(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            new ServiceCollection().BuildServiceProvider().GetRequiredService<IServiceScopeFactory>(),
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new TestWebSocket(),
            state,
            TestContext.Current.CancellationToken);

        Assert.True(broadcasterQueueStillPresentWhenRuntimeRemoved);
        Assert.Null(runtimes.TryGet(sessionId));
        Assert.Null(broadcaster.TryGetSession(sessionId));
    }

    [Fact]
    public async Task Stale_Participant_Teardown_Preserves_Republished_Queues_And_Capacity()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "participant-incarnation",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var oldStartedAt = session.StartedAt;
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var oldRuntime = runtimes.CreateRuntime(session.Id);
        oldRuntime.Host.SetSessionStartedAt(oldStartedAt);
        oldRuntime.Participants.TryAddSharedParticipant(
            "old-viewer",
            50,
            out _);
        var oldQueues = broadcaster.GetOrCreateSession(session.Id);
        var oldParticipantQueue = oldQueues.AddParticipantQueue("old-viewer");
        var connections = new ConnectionRegistry();
        var viewerId = UserId.New();
        connections.RegisterSharedParticipant(
            "old-viewer",
            viewerId,
            "old-device",
            session.Id);
        var repository = new IncarnationCountRepository(session, initialCount: 1);
        var state = new ParticipantConnectionState(
            "old-viewer",
            session.Id.Value.ToString(),
            viewerId)
        {
            SessionId = session.Id,
            Ports = oldRuntime,
            SessionQueues = oldQueues,
            ParticipantQueue = oldParticipantQueue,
            ParticipantRegistered = true,
            DbParticipantCounted = true,
            AccessDecision = new ParticipantAccessDecision(
                IsOwnerParticipant: false,
                AccessLevel.View,
                oldStartedAt),
        };

        Assert.True(runtimes.RemoveIfSame(session.Id, oldRuntime));
        Assert.True(broadcaster.RemoveSessionIfSame(session.Id, oldQueues));
        session.End();
        await Task.Delay(1, TestContext.Current.CancellationToken);
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            session.OwnerUserId,
            "republished-participant",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "new-secret",
            roomId: null);
        repository.SetCount(50);
        var replacementRuntime = runtimes.CreateRuntime(session.Id);
        replacementRuntime.Host.SetSessionStartedAt(session.StartedAt);
        var replacementQueues = broadcaster.GetOrCreateSession(session.Id);
        var replacementQueue =
            replacementQueues.AddParticipantQueue("new-viewer");
        var teardown = new ParticipantTeardown(
            connections,
            runtimes,
            new SingleRepositoryScopeFactory(repository),
            broadcaster,
            metrics,
            new SessionLifecycleGate(),
            NullLogger<ParticipantTeardown>.Instance,
            new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new TestWebSocket(),
            state,
            TestContext.Current.CancellationToken);

        Assert.Equal(50, repository.Count);
        Assert.False(await repository.TryIncrementParticipantCountAsync(
            session.Id,
            session.StartedAt,
            50,
            TestContext.Current.CancellationToken));
        Assert.Same(replacementRuntime, runtimes.TryGet(session.Id));
        Assert.Same(replacementQueues, broadcaster.TryGetSession(session.Id));
        Assert.Null(replacementQueue.CompletionCause);
    }

    [Fact]
    public async Task Delayed_Old_Teardown_Does_Not_Decrement_Replacement_Runtime_Count()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "replacement-runtime-count",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var oldRuntime = runtimes.CreateRuntime(session.Id);
        oldRuntime.Host.SetSessionStartedAt(session.StartedAt);
        oldRuntime.Participants.TryAddSharedParticipant("old-viewer", 50, out _);
        var oldQueues = broadcaster.GetOrCreateSession(session.Id);
        var oldQueue = oldQueues.AddParticipantQueue("old-viewer");
        var viewerId = UserId.New();
        var connections = new ConnectionRegistry();
        connections.RegisterSharedParticipant(
            "old-viewer",
            viewerId,
            "old-device",
            session.Id);
        var state = new ParticipantConnectionState(
            "old-viewer",
            session.Id.Value.ToString(),
            viewerId)
        {
            SessionId = session.Id,
            Ports = oldRuntime,
            SessionQueues = oldQueues,
            ParticipantQueue = oldQueue,
            ParticipantRegistered = true,
            DbParticipantCounted = true,
            AccessDecision = new ParticipantAccessDecision(
                IsOwnerParticipant: false,
                AccessLevel.View,
                session.StartedAt),
        };
        Assert.True(runtimes.RemoveIfSame(session.Id, oldRuntime));
        Assert.True(broadcaster.RemoveSessionIfSame(session.Id, oldQueues));
        var replacementRuntime = runtimes.CreateRuntime(session.Id);
        replacementRuntime.Host.SetSessionStartedAt(session.StartedAt);
        var replacementQueues = broadcaster.GetOrCreateSession(session.Id);
        var replacementQueue =
            replacementQueues.AddParticipantQueue("replacement-viewer");
        var repository = new IncarnationCountRepository(
            session,
            initialCount: 50);
        var teardown = new ParticipantTeardown(
            connections,
            runtimes,
            new SingleRepositoryScopeFactory(repository),
            broadcaster,
            metrics,
            new SessionLifecycleGate(),
            NullLogger<ParticipantTeardown>.Instance,
            new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new TestWebSocket(),
            state,
            TestContext.Current.CancellationToken);

        Assert.Equal(50, repository.Count);
        Assert.Same(replacementRuntime, runtimes.TryGet(session.Id));
        Assert.Same(replacementQueues, broadcaster.TryGetSession(session.Id));
        Assert.Null(replacementQueue.CompletionCause);
    }

    [Fact]
    public async Task Rejected_Handshake_Teardown_Releases_Unregistered_Reservation_And_Owned_Queue()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "rejected-participant",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var runtime = runtimes.CreateRuntime(session.Id);
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        Assert.True(runtime.Participants.TryAddSharedParticipant(
            "rejected-viewer",
            50,
            out _));
        var queues = broadcaster.GetOrCreateSession(session.Id);
        var participantQueue = queues.AddParticipantQueue(
            "rejected-viewer");
        var repository = new IncarnationCountRepository(
            session,
            initialCount: 1);
        var state = new ParticipantConnectionState(
            "rejected-viewer",
            session.Id.Value.ToString(),
            UserId.New())
        {
            SessionId = session.Id,
            Ports = runtime,
            SessionQueues = queues,
            ParticipantQueue = participantQueue,
            ParticipantReserved = true,
            DbParticipantCounted = true,
            AccessDecision = new ParticipantAccessDecision(
                IsOwnerParticipant: false,
                AccessLevel.View,
                session.StartedAt),
        };
        var teardown = new ParticipantTeardown(
            new ConnectionRegistry(),
            runtimes,
            new SingleRepositoryScopeFactory(repository),
            broadcaster,
            metrics,
            new SessionLifecycleGate(),
            NullLogger<ParticipantTeardown>.Instance,
            new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new TestWebSocket(),
            state,
            TestContext.Current.CancellationToken);

        Assert.Equal(0, repository.Count);
        Assert.Equal(
            0,
            runtime.Demand.GetStreamDemand().SharedParticipantCount);
        Assert.Null(queues.GetParticipantQueue(state.ConnectionId));
        Assert.False(state.ParticipantReserved);
        Assert.False(state.DbParticipantCounted);
    }

    [Fact]
    public async Task ParticipantTeardown_Emits_Participant_Disconnected_To_Host_Queue()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(runtimes, metrics, NullLoggerFactory.Instance);
        var sessionId = SessionId.New();
        var ports = runtimes.CreateRuntime(sessionId);
        ports.Host.TryClaimHost("host-1", new CancellationTokenSource());
        ports.Host.SetHostReady(true);
        ports.Host.SetStatus(SessionStatus.Live);

        var queues = broadcaster.GetOrCreateSession(sessionId);
        var hostQueue = queues.SetHostQueue();
        var participantQueue = queues.AddParticipantQueue("viewer-7");
        var participantUserId = UserId.New();
        Assert.True(ports.Participants.TryAddSharedParticipant(
            "viewer-7",
            maxSharedParticipantCount: 8,
            out _));
        var connections = new ConnectionRegistry();
        connections.RegisterSharedParticipant("viewer-7", participantUserId, "viewer-device", sessionId);

        var state = new ParticipantConnectionState("viewer-7", sessionId.Value.ToString(), participantUserId)
        {
            SessionId = sessionId,
            Ports = ports,
            SessionQueues = queues,
            ParticipantQueue = participantQueue,
            ParticipantRegistered = true,
            AccessDecision = new ParticipantAccessDecision(IsOwnerParticipant: false, AccessLevel.Inject),
        };

        var teardown = new ParticipantTeardown(
            connections,
            runtimes,
            new SingleRepositoryScopeFactory(new FakeSessionRepository()),
            broadcaster,
            metrics,
            new SessionLifecycleGate(),
            NullLogger<ParticipantTeardown>.Instance,
            new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new TestWebSocket(),
            state,
            TestContext.Current.CancellationToken);

        var hostMessage = await hostQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(hostMessage);
        var disconnected = JsonSerializer.Deserialize(
            hostMessage!,
            WsJsonContext.Default.HostParticipantDisconnectedMessage);
        Assert.NotNull(disconnected);
        Assert.Equal("viewer-7", disconnected!.ClientId);
        Assert.Equal(sessionId.Value.ToString(), disconnected.SessionId);
        Assert.Equal("host.participantDisconnected", disconnected.Type);




        Assert.DoesNotContain("host.focusChanged", Encoding.UTF8.GetString(hostMessage!));
    }

    private static ParticipantTeardown CreateParticipantTeardown()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(runtimes, metrics, NullLoggerFactory.Instance);
        return new ParticipantTeardown(
            new ConnectionRegistry(),
            runtimes,
            new SingleRepositoryScopeFactory(new FakeSessionRepository()),
            broadcaster,
            metrics,
            new SessionLifecycleGate(),
            NullLogger<ParticipantTeardown>.Instance,
            new RealtimePersistenceRepairTracker(TimeProvider.System));
    }

    private sealed class IncarnationCountRepository(
        Session session,
        int initialCount) : SessionRepositoryStub
    {
        public int Count { get; private set; } = initialCount;

        public void SetCount(int count) => Count = count;

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
    }

    private sealed class SingleRepositoryScopeFactory(
        ISessionRepository sessions) : IServiceScopeFactory
    {
        private readonly ISessionRepository _sessions = sessions;

        public IServiceScope CreateScope() =>
            new SingleRepositoryScope(_sessions);
    }

    private sealed class SingleRepositoryScope(
        ISessionRepository sessions) : IServiceScope
    {
        public IServiceProvider ServiceProvider { get; } =
            new SingleRepositoryServiceProvider(sessions);

        public void Dispose()
        {
        }
    }

    private sealed class SingleRepositoryServiceProvider(
        ISessionRepository sessions) : IServiceProvider
    {
        private readonly ISessionRepository _sessions = sessions;

        public object? GetService(Type serviceType) =>
            serviceType == typeof(ISessionRepository) ? _sessions : null;
    }

    private sealed class ObservingRuntimeDirectory : ILiveSessionStateDirectory
    {
        private readonly LiveSessionStateDirectory _inner = new();

        public Action<SessionId>? OnRemove { get; set; }

        public LiveSessionPorts? TryGet(SessionId sessionId) =>
            _inner.TryGet(sessionId);

        public bool TryClaimHost(
            SessionId sessionId,
            string connectionId,
            CancellationTokenSource hostLifetime,
            out LiveSessionPorts? ports,
            out LiveSessionCreationOwnership? creationOwnership) =>
            _inner.TryClaimHost(
                sessionId,
                connectionId,
                hostLifetime,
                out ports,
                out creationOwnership);

        public bool RemoveIfSame(SessionId sessionId, LiveSessionPorts expected)
        {
            var removed = _inner.RemoveIfSame(sessionId, expected);
            if (removed)
            {
                OnRemove?.Invoke(sessionId);
            }
            return removed;
        }

        public bool RemoveIfOwned(
            SessionId sessionId,
            LiveSessionCreationOwnership creationOwnership)
        {
            var removed = _inner.RemoveIfOwned(
                sessionId,
                creationOwnership);
            if (removed)
            {
                OnRemove?.Invoke(sessionId);
            }
            return removed;
        }

        public IReadOnlyList<SessionId> GetActiveSessions() =>
            _inner.GetActiveSessions();

    }

    private sealed class TestWebSocket : WebSocket
    {
        public override WebSocketCloseStatus? CloseStatus => LastCloseStatus;
        public override string? CloseStatusDescription => LastCloseDescription;
        public override WebSocketState State { get; } = WebSocketState.Open;
        public override string? SubProtocol => null;
        public WebSocketCloseStatus? LastCloseStatus { get; private set; }
        public string? LastCloseDescription { get; private set; }

        public override void Abort() { }

        public override Task CloseAsync(WebSocketCloseStatus closeStatus, string? statusDescription, CancellationToken cancellationToken)
        {
            LastCloseStatus = closeStatus;
            LastCloseDescription = statusDescription;
            return Task.CompletedTask;
        }

        public override Task CloseOutputAsync(WebSocketCloseStatus closeStatus, string? statusDescription, CancellationToken cancellationToken)
        {
            LastCloseStatus = closeStatus;
            LastCloseDescription = statusDescription;
            return Task.CompletedTask;
        }

        public override void Dispose() { }

        public override Task<WebSocketReceiveResult> ReceiveAsync(ArraySegment<byte> buffer, CancellationToken cancellationToken) =>
            throw new NotSupportedException();

        public override Task SendAsync(
            ArraySegment<byte> buffer,
            WebSocketMessageType messageType,
            bool endOfMessage,
            CancellationToken cancellationToken) =>
            throw new NotSupportedException();
    }
}
