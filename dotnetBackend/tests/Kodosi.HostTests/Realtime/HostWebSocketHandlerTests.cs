using System.Net.WebSockets;
using System.Text.Json;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Logging.Abstractions;
using Microsoft.EntityFrameworkCore;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Auth;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;
using Kodosi.Infrastructure.Crypto;
using Kodosi.Infrastructure.Persistence;

using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

public sealed class HostWebSocketHandlerTests
{
    private const string HostDeviceId = "host-device";

    [Theory]
    [InlineData(3)]
    [InlineData(4)]
    [InlineData(5)]
    [InlineData(6)]
    [InlineData(7)]
    [InlineData(8)]
    public async Task HostHandshake_Rejects_Noncurrent_Relay_Protocol(int version)
    {
        var session = CreatePendingSession();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var hello = JsonSerializer.Serialize(
            new HostHelloMessage(
                session.Id.Value.ToString(),
                "owner-secret",
                HostDeviceId,
                ExpectedIncarnationId: session.IncarnationId,
                RelayProtocolVersion: version),
            WsJsonContext.Default.HostHelloMessage);
        using var webSocket = new ScriptedWebSocket(
            [Frame.Text(System.Text.Encoding.UTF8.GetBytes(hello), endOfMessage: true)]);
        var state = new HostConnectionState(
            "wrong-protocol-host",
            session.Id.Value.ToString(),
            session.OwnerUserId);
        var handshake = new HostHandshake(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            new RealtimeDeviceAuthorizationReader(
                services.GetRequiredService<IServiceScopeFactory>(),
                TimeProvider.System),
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostHandshake>.Instance);

        Assert.False(await handshake.TryAcceptAsync(
            webSocket,
            state,
            TestContext.Current.CancellationToken));
        Assert.False(state.HostClaimed);
        Assert.Equal("unsupported_data", webSocket.CloseStatusDescription);
        state.DisposeDeviceAuthorizationLifetime();
    }

    [Theory]
    [InlineData("arbitrary.message", true)]
    [InlineData("device.proof", false)]
    public async Task HostHandshake_Rejects_Proof_Discriminator_Or_Hello_Session_Drift(
        string proofType,
        bool helloMatchesRoute)
    {
        var session = CreatePendingSession(currentIncarnation: true);
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var hello = JsonSerializer.Serialize(
            new HostHelloMessage(
                helloMatchesRoute
                    ? session.Id.Value.ToString()
                    : Guid.NewGuid().ToString(),
                "owner-secret",
                HostDeviceId,
                ExpectedIncarnationId: session.IncarnationId,
                RelayProtocolVersion: RelayProtocolVersions.Current),
            WsJsonContext.Default.HostHelloMessage);
        using var webSocket = new ScriptedWebSocket(
            [Frame.Text(System.Text.Encoding.UTF8.GetBytes(hello), endOfMessage: true)],
            proofType: proofType,
            proofSessionId: session.Id.Value.ToString("D").ToLowerInvariant());
        var state = new HostConnectionState(
            "invalid-proof-binding-host",
            session.Id.Value.ToString(),
            session.OwnerUserId);
        var handshake = new HostHandshake(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            new RealtimeDeviceAuthorizationReader(
                services.GetRequiredService<IServiceScopeFactory>(),
                TimeProvider.System),
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostHandshake>.Instance);

        Assert.False(await handshake.TryAcceptAsync(
            webSocket,
            state,
            TestContext.Current.CancellationToken));
        Assert.False(state.HostClaimed);
        Assert.Null(runtimes.TryGet(session.Id));
        Assert.Equal("access_revoked", webSocket.CloseStatusDescription);
        state.DisposeDeviceAuthorizationLifetime();
    }

    [Fact]
    public async Task CurrentIncarnation_Rejects_Host_Without_Expected_Before_Runtime_Claim()
    {
        var session = CreatePendingSession(currentIncarnation: true);
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var hello = JsonSerializer.Serialize(
            new HostHelloMessage(
                session.Id.Value.ToString(),
                "owner-secret",
                HostDeviceId,
                ExpectedIncarnationId: null,
                RelayProtocolVersion: RelayProtocolVersions.Current),
            WsJsonContext.Default.HostHelloMessage);
        using var webSocket = new ScriptedWebSocket(
            [Frame.Text(System.Text.Encoding.UTF8.GetBytes(hello), endOfMessage: true)]);
        var state = new HostConnectionState(
            "missing-incarnation-host",
            session.Id.Value.ToString(),
            session.OwnerUserId);
        var handshake = new HostHandshake(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            new RealtimeDeviceAuthorizationReader(
                services.GetRequiredService<IServiceScopeFactory>(),
                TimeProvider.System),
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostHandshake>.Instance);

        Assert.False(await handshake.TryAcceptAsync(
            webSocket,
            state,
            TestContext.Current.CancellationToken));
        Assert.Null(runtimes.TryGet(session.Id));
        Assert.Null(session.HostConnectionSlot);
        Assert.False(state.HostClaimed);
        state.DisposeDeviceAuthorizationLifetime();
    }

    [Fact]
    public async Task HostHandshake_Accepts_Hello_Above_Legacy_8KiB_And_Within_Authority_Limit()
    {
        var secret = new string('s', 12 * 1024);
        var session = CreatePendingSession(secret, currentIncarnation: true);
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var scopedLifetime = new ScopedLifetimeProbe();
        var dbContexts = new List<KodosiDbContext>();
        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId,
            scopedLifetime: scopedLifetime,
            dbContexts: dbContexts);
        var hello = JsonSerializer.Serialize(
            new HostHelloMessage(
                session.Id.Value.ToString(),
                secret,
                HostDeviceId,
                ExpectedIncarnationId: session.IncarnationId,
                RelayProtocolVersion: RelayProtocolVersions.Current),
            WsJsonContext.Default.HostHelloMessage);
        var helloBytes = System.Text.Encoding.UTF8.GetByteCount(hello);
        Assert.InRange(
            helloBytes,
            (8 * 1024) + 1,
            RelayMessageLimits.GetMaxBytes("host.hello"));
        var splitAt = hello.Length / 2;
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(System.Text.Encoding.UTF8.GetBytes(hello[..splitAt])),
                Frame.Text(
                    System.Text.Encoding.UTF8.GetBytes(hello[splitAt..]),
                    endOfMessage: true),
            ]);
        var state = new HostConnectionState(
            "large-hello-host",
            session.Id.Value.ToString(),
            session.OwnerUserId);
        var handshake = new HostHandshake(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            new RealtimeDeviceAuthorizationReader(
                services.GetRequiredService<IServiceScopeFactory>(),
                TimeProvider.System),
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostHandshake>.Instance);

        var accepted = await handshake.TryAcceptAsync(
            webSocket,
            state,
            TestContext.Current.CancellationToken);

        Assert.True(accepted);
        Assert.True(state.HostAccepted);
        Assert.Equal(WebSocketState.Open, webSocket.State);
        Assert.True(scopedLifetime.Created > 0);
        Assert.Equal(scopedLifetime.Created, scopedLifetime.Disposed);
        Assert.NotEmpty(dbContexts);
        Assert.All(dbContexts, dbContext =>
            Assert.Throws<ObjectDisposedException>(() => dbContext.Entry(session)));
        state.LinkedCts?.Dispose();
        state.DisposeDeviceAuthorizationLifetime();
    }

    [Fact]
    public async Task PreAccepted_Live_Failure_Is_Reconciled_To_Reconnecting()
    {
        var session = CreatePendingSession();
        session.ActivateHost("host-1");
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var ports = runtimes.CreateRuntime(session.Id);
        using var hostLifetime = new CancellationTokenSource();
        Assert.True(ports.Host.TryClaimHost("host-1", hostLifetime));
        var queues = broadcaster.GetOrCreateSession(session.Id);
        queues.SetHostQueue("host-1");
        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var state = new HostConnectionState(
            "host-1",
            session.Id.Value.ToString(),
            session.OwnerUserId)
        {
            SessionId = session.Id,
            Ports = ports,
            SessionQueues = queues,
            HostClaimed = true,
            PersistedLiveBeforeAcceptance = true,
            ActivatedSessionStartedAt = session.StartedAt,
            ActivatedSessionIncarnationId = session.IncarnationId,
            ActivatedRuntimeIncarnationId = ports.IncarnationId,
        };
        var teardown = new HostTeardown(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new ScriptedWebSocket([]),
            state,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionStatus.Reconnecting, session.Status);
    }

    [Fact]
    public async Task Republished_Session_Rejects_Previous_Incarnation_Secret_Atomically()
    {
        var session = CreatePendingSession("old-secret");
        session.End();
        await Task.Delay(1, TestContext.Current.CancellationToken);
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            session.OwnerUserId,
            "republished",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            new OwnerSessionSecretHasher().Hash("new-secret"),
            roomId: null);
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var handshake = new HostHandshake(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            new RealtimeDeviceAuthorizationReader(scopeFactory, TimeProvider.System),
            scopeFactory,
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostHandshake>.Instance);
        var state = new HostConnectionState(
            "old-secret-host",
            session.Id.Value.ToString(),
            session.OwnerUserId);
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "old-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);

        Assert.False(await handshake.TryAcceptAsync(
            webSocket,
            state,
            TestContext.Current.CancellationToken));
        Assert.Equal(
            CloseReason.InvalidSessionOrSecret.ToWire(),
            webSocket.LastCloseDescription);
        Assert.Equal(SessionStatus.Pending, session.Status);
        Assert.Null(session.HostConnectionSlot);
        Assert.Null(runtimes.TryGet(session.Id));
    }

    [Fact]
    public async Task Teardown_Removes_Registry_Entry_When_Host_Registered_But_Never_Accepted()
    {






        var session = CreatePendingSession();
        session.ActivateHost("host-1");
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var ports = runtimes.CreateRuntime(session.Id);
        using var hostLifetime = new CancellationTokenSource();
        Assert.True(ports.Host.TryClaimHost("host-1", hostLifetime));
        var queues = broadcaster.GetOrCreateSession(session.Id);
        queues.SetHostQueue("host-1");
        var connections = new ConnectionRegistry();
        connections.RegisterHost(
            "host-1",
            session.OwnerUserId,
            HostDeviceId,
            session.Id);
        Assert.Equal(1, connections.ActiveConnectionCount);

        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var state = new HostConnectionState(
            "host-1",
            session.Id.Value.ToString(),
            session.OwnerUserId)
        {
            SessionId = session.Id,
            Ports = ports,
            SessionQueues = queues,
            HostClaimed = true,
            HostRegistered = true,
            HostAccepted = false,
            PersistedLiveBeforeAcceptance = true,
            ActivatedSessionStartedAt = session.StartedAt,
            ActivatedSessionIncarnationId = session.IncarnationId,
            ActivatedRuntimeIncarnationId = ports.IncarnationId,
        };
        var teardown = new HostTeardown(
            connections,
            runtimes,
            broadcaster,
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new ScriptedWebSocket([]),
            state,
            TestContext.Current.CancellationToken);

        Assert.Equal(0, connections.ActiveConnectionCount);
    }

    [Fact]
    public async Task Delayed_PreAccepted_Teardown_Does_Not_Reconnect_Replacement_Incarnation()
    {
        var session = CreatePendingSession();
        session.ActivateHost("old-host");
        var oldStartedAt = session.StartedAt;
        var oldIncarnationId = session.IncarnationId;
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var oldRuntime = runtimes.CreateRuntime(session.Id, out var oldCreationOwnership);
        using var oldHostLifetime = new CancellationTokenSource();
        Assert.True(oldRuntime.Host.TryClaimHost("old-host", oldHostLifetime));
        var oldQueues = broadcaster.GetOrCreateSession(session.Id);
        oldQueues.SetHostQueue("old-host");

        Assert.True(runtimes.RemoveIfSame(session.Id, oldRuntime));
        Assert.True(broadcaster.RemoveSessionIfSame(
            session.Id,
            oldQueues,
            CloseReason.ClosingNormal));
        session.End();
        session.ReleaseHostSlot("old-host");
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            session.OwnerUserId,
            "replacement",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            new OwnerSessionSecretHasher().Hash("replacement-secret"),
            roomId: null);
        session.ActivateHost("replacement-host");
        var replacementRuntime = runtimes.CreateRuntime(session.Id);
        replacementRuntime.Host.SetStatus(SessionStatus.Live);
        using var replacementHostLifetime = new CancellationTokenSource();
        Assert.True(replacementRuntime.Host.TryClaimHost(
            "replacement-host",
            replacementHostLifetime));
        var replacementQueues = broadcaster.GetOrCreateSession(session.Id);
        var replacementHostQueue = replacementQueues.SetHostQueue("replacement-host");

        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var state = new HostConnectionState(
            "old-host",
            session.Id.Value.ToString(),
            session.OwnerUserId)
        {
            SessionId = session.Id,
            Ports = oldRuntime,
            SessionQueues = oldQueues,
            RuntimeCreationOwnership = oldCreationOwnership,
            HostClaimed = true,
            HostAccepted = false,
            PersistedLiveBeforeAcceptance = true,
            ActivatedSessionStartedAt = oldStartedAt,
            ActivatedSessionIncarnationId = oldIncarnationId,
            ActivatedRuntimeIncarnationId = oldRuntime.IncarnationId,
            DurableSlotReleaseRequired = true,
        };
        var teardown = new HostTeardown(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new ScriptedWebSocket([]),
            state,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionStatus.Live, session.Status);
        Assert.Equal("replacement-host", session.HostConnectionSlot);
        Assert.Same(replacementRuntime, runtimes.TryGet(session.Id));
        Assert.Same(replacementQueues, broadcaster.TryGetSession(session.Id));
        Assert.Null(replacementHostQueue.CompletionReason);
    }

    [Fact]
    public async Task Teardown_Does_Not_Reopen_Session_Ended_By_Other_Entry_Path()
    {
        var session = CreatePendingSession();
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var ports = runtimes.CreateRuntime(session.Id);
        ports.Host.SetStatus(SessionStatus.Ended);
        using var hostLifetime = new CancellationTokenSource();
        Assert.True(ports.Host.TryClaimHost("host-1", hostLifetime));
        var queues = broadcaster.GetOrCreateSession(session.Id);
        queues.SetHostQueue("host-1");
        var participant = queues.AddParticipantQueue("participant-1");
        broadcaster.BroadcastSessionEnded(session.Id, CloseReason.HostStopped);
        using var services = CreateScopedServices(
            new ThrowOnTransitionSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var state = new HostConnectionState(
            "host-1",
            session.Id.Value.ToString(),
            session.OwnerUserId)
        {
            SessionId = session.Id,
            Ports = ports,
            SessionQueues = queues,
            HostClaimed = true,
            HostAccepted = true,
            CloseReason = CloseReason.HostStopped,
        };
        var teardown = new HostTeardown(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new ScriptedWebSocket([]),
            state,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionStatus.Ended, ports.Host.Status);
        Assert.Null(runtimes.TryGet(session.Id));
        Assert.Null(broadcaster.TryGetSession(session.Id));
        var endedBytes = await participant.ReadAsync(CancellationToken.None);
        var ended = JsonSerializer.Deserialize(
            endedBytes!.Value.Payload,
            WsJsonContext.Default.SessionEndedMessage);
        Assert.Equal("host_stopped", ended?.Reason);
        Assert.Null(await participant.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task Stale_Old_Host_Teardown_Preserves_Republished_Runtime_And_Queues()
    {
        var session = CreatePendingSession();
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var lifecycleGate = new SessionLifecycleGate();
        var oldRuntime = runtimes.CreateRuntime(session.Id);
        oldRuntime.Host.SetStatus(SessionStatus.Ended);
        using var oldHostLifetime = new CancellationTokenSource();
        Assert.True(oldRuntime.Host.TryClaimHost("old-host", oldHostLifetime));
        var oldQueues = broadcaster.GetOrCreateSession(session.Id);
        oldQueues.SetHostQueue("old-host");
        broadcaster.BroadcastSessionEnded(session.Id, CloseReason.HostStopped);
        Assert.True(runtimes.RemoveIfSame(session.Id, oldRuntime));
        Assert.True(broadcaster.RemoveSessionIfSame(
            session.Id,
            oldQueues,
            CloseReason.HostStopped));

        session.End();
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            session.OwnerUserId,
            "republished-session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            new OwnerSessionSecretHasher().Hash("new-owner-secret"),
            roomId: null);
        var replacementRuntime = runtimes.CreateRuntime(session.Id);
        replacementRuntime.Host.SetStatus(SessionStatus.Pending);
        var replacementQueues = broadcaster.GetOrCreateSession(session.Id);
        var replacementHostQueue = replacementQueues.SetHostQueue("replacement-host");
        using var services = CreateScopedServices(
            new ThrowOnTransitionSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var state = new HostConnectionState(
            "old-host",
            session.Id.Value.ToString(),
            session.OwnerUserId)
        {
            SessionId = session.Id,
            Ports = oldRuntime,
            SessionQueues = oldQueues,
            HostClaimed = true,
            HostAccepted = true,
        };
        var teardown = new HostTeardown(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            services.GetRequiredService<IServiceScopeFactory>(),
            metrics,
            lifecycleGate,
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));

        await teardown.RunAsync(
            new ScriptedWebSocket([]),
            state,
            TestContext.Current.CancellationToken);

        Assert.Same(replacementRuntime, runtimes.TryGet(session.Id));
        Assert.Same(replacementQueues, broadcaster.TryGetSession(session.Id));
        Assert.Null(replacementHostQueue.CompletionReason);
    }

    [Fact]
    public async Task HandleAsync_Rejects_When_DB_Host_Slot_Is_Already_Claimed()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var session = CreatePendingSession();
        SetHostSlot(session, "existing-host");
        using var services = CreateScopedServices(
            new DbSlotClaimRejectingRepository(session),
            metrics,
            session.OwnerUserId);
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var messageProcessor = CreateMessageProcessor(
            runtimeDirectory,
            broadcaster,
            scopeFactory,
            metrics);
        var handler = CreateHandler(
            connectionRegistry,
            runtimeDirectory,
            broadcaster,
            scopeFactory,
            metrics,
            messageProcessor,
            new AlwaysValidJwtRevalidator(),
            NullLogger<HostWebSocketHandler>.Instance);
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "owner-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);

        await handler.HandleAsync(
            webSocket,
            session.Id.Value.ToString(),
            session.OwnerUserId,
            accessToken: null,
            CancellationToken.None);

        Assert.Equal(WebSocketCloseStatus.PolicyViolation, webSocket.LastCloseStatus);
        Assert.Equal("already_hosted", webSocket.LastCloseDescription);
        Assert.Null(runtimeDirectory.TryGet(session.Id));
    }

    [Fact]
    public async Task Host_Rejection_Releases_Transaction_And_Lifecycle_Before_Bounded_Close()
    {
        var runtimes = new LiveSessionStateDirectory();
        var connections = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var lifecycleGate = new SessionLifecycleGate();
        var session = CreatePendingSession();
        var existingRuntime = runtimes.CreateRuntime(session.Id);
        Assert.True(existingRuntime.Host.TryClaimHost(
            "existing-host",
            new CancellationTokenSource()));
        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var teardown = new HostTeardown(
            connections,
            runtimes,
            broadcaster,
            scopeFactory,
            metrics,
            lifecycleGate,
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));
        var coordinator = new SessionEndCoordinator(
            scopeFactory,
            runtimes,
            broadcaster,
            teardown,
            lifecycleGate,
            NullLogger<SessionEndCoordinator>.Instance);
        var processor = new HostMessageProcessor(
            broadcaster,
            coordinator,
            lifecycleGate,
            runtimes,
            metrics,
            NullLogger<HostMessageProcessor>.Instance,
            UnsupportedSemanticRelayRepository.Instance,
            connections,
            new ActionDedupeCache(),
            UnsupportedPermissionDecisionAuditStore.Instance,
            scopeFactory);
        var handler = new HostWebSocketHandler(
            new HostHandshake(
                connections,
                runtimes,
                broadcaster,
                new RealtimeDeviceAuthorizationReader(scopeFactory, TimeProvider.System),
                scopeFactory,
                metrics,
                lifecycleGate,
                NullLogger<HostHandshake>.Instance,
                handshakeTimeout: TimeSpan.FromMilliseconds(25)),
            new HostSessionPump(
                broadcaster,
                processor,
                new AlwaysValidJwtRevalidator(),
                metrics,
                NullLogger<HostSessionPump>.Instance),
            teardown,
            NullLogger<HostWebSocketHandler>.Instance,
            TimeProvider.System);
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "owner-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ],
            blockCloseOutput: true);

        var handling = handler.HandleAsync(
            webSocket,
            session.Id.Value.ToString(),
            session.OwnerUserId,
            accessToken: null,
            TestContext.Current.CancellationToken);
        await webSocket.CloseOutputStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        await using (await lifecycleGate.AcquireAsync(
            session.Id,
            TestContext.Current.CancellationToken))
        {
            Assert.Equal(1, lifecycleGate.TestActiveSessionCount());
        }
        await handling.WaitAsync(
            TimeSpan.FromSeconds(1),
            TestContext.Current.CancellationToken);

        Assert.Equal(WebSocketState.Aborted, webSocket.State);
        Assert.Null(session.HostConnectionSlot);
        Assert.Equal(0, connections.ActiveConnectionCount);
        Assert.Equal(0, lifecycleGate.TestActiveSessionCount());
    }

    [Fact]
    public async Task AlreadyClaimed_Runtime_Prevents_Durable_Live_Commit()
    {
        var session = CreatePendingSession();
        var runtimes = new LiveSessionStateDirectory();
        var runtime = runtimes.CreateRuntime(session.Id);
        Assert.True(runtime.Host.TryClaimHost(
            "existing-runtime-host",
            new CancellationTokenSource()));
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var handshake = new HostHandshake(
            new ConnectionRegistry(),
            runtimes,
            broadcaster,
            new RealtimeDeviceAuthorizationReader(scopeFactory, TimeProvider.System),
            scopeFactory,
            metrics,
            new SessionLifecycleGate(),
            NullLogger<HostHandshake>.Instance);
        var state = new HostConnectionState(
            "racing-host",
            session.Id.Value.ToString(),
            session.OwnerUserId);
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "owner-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);

        Assert.False(await handshake.TryAcceptAsync(
            webSocket,
            state,
            TestContext.Current.CancellationToken));

        Assert.Equal(CloseReason.AlreadyHosted.ToWire(), webSocket.LastCloseDescription);
        Assert.Equal(SessionStatus.Pending, session.Status);
        Assert.Null(session.HostConnectionSlot);
        Assert.Equal("existing-runtime-host", runtime.Host.HostConnectionId);
    }

    [Fact]
    public async Task Failed_Activation_Cleanup_Cannot_Remove_Concurrent_Valid_Host_Queues()
    {
        var session = CreatePendingSession();
        using var runtimes = new BlockingNullObservationRuntimeDirectory();
        var connections = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var staleQueues = broadcaster.GetOrCreateSession(session.Id);
        staleQueues.AddParticipantQueue("waiting-participant");
        var lifecycleGate = new SessionLifecycleGate();
        using var services = CreateScopedServices(
            new FakeSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var handshake = new HostHandshake(
            connections,
            runtimes,
            broadcaster,
            new RealtimeDeviceAuthorizationReader(scopeFactory, TimeProvider.System),
            scopeFactory,
            metrics,
            lifecycleGate,
            NullLogger<HostHandshake>.Instance);
        var invalidState = new HostConnectionState(
            "invalid-host",
            session.Id.Value.ToString(),
            session.OwnerUserId);
        var validState = new HostConnectionState(
            "valid-host",
            session.Id.Value.ToString(),
            session.OwnerUserId);
        using var invalidSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "wrong-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);
        using var validSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "owner-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);

        var invalidAccepting = Task.Run(() => handshake.TryAcceptAsync(
            invalidSocket,
            invalidState,
            TestContext.Current.CancellationToken));
        await runtimes.NullObservationStarted.WaitAsync(
            TestContext.Current.CancellationToken);
        var validAccepting = Task.Run(() => handshake.TryAcceptAsync(
            validSocket,
            validState,
            TestContext.Current.CancellationToken));
        var validCompletedBeforeCleanup = await Task.WhenAny(
                validAccepting,
                Task.Delay(50, TestContext.Current.CancellationToken))
            == validAccepting;

        runtimes.ReleaseNullObservation();
        Assert.False(await invalidAccepting);
        Assert.True(await validAccepting);

        Assert.False(validCompletedBeforeCleanup);
        Assert.NotSame(staleQueues, validState.SessionQueues);
        Assert.Same(validState.Ports, runtimes.TryGet(session.Id));
        Assert.Same(validState.SessionQueues, broadcaster.TryGetSession(session.Id));
        Assert.Null(validState.HostQueue?.CompletionReason);

        invalidState.LinkedCts?.Dispose();
        invalidState.DisposeDeviceAuthorizationLifetime();
        validState.LinkedCts?.Dispose();
        validState.DisposeDeviceAuthorizationLifetime();
    }

    [Theory]
    [InlineData(HostActivationFailurePoint.Save)]
    [InlineData(HostActivationFailurePoint.Commit)]
    public async Task Activation_Exception_Compensates_Runtime_And_Durable_Claims_Exactly_Once(
        HostActivationFailurePoint failurePoint)
    {
        var session = CreatePendingSession();
        var sessions = new FakeSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        var connections = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var preActivationQueues = broadcaster.GetOrCreateSession(session.Id);
        var preActivationParticipant =
            preActivationQueues.AddParticipantQueue("waiting-participant");
        using var services = CreateScopedServices(
            sessions,
            metrics,
            session.OwnerUserId,
            unitOfWork: new FailingOnceUnitOfWork(failurePoint));
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var handler = CreateHandler(
            connections,
            runtimes,
            broadcaster,
            scopeFactory,
            metrics,
            CreateMessageProcessor(
                runtimes,
                broadcaster,
                scopeFactory,
                metrics),
            new AlwaysValidJwtRevalidator(),
            NullLogger<HostWebSocketHandler>.Instance);
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "owner-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);

        await handler.HandleAsync(
            webSocket,
            session.Id.Value.ToString(),
            session.OwnerUserId,
            accessToken: null,
            CancellationToken.None);

        Assert.Null(runtimes.TryGet(session.Id));
        Assert.Null(broadcaster.TryGetSession(session.Id));
        Assert.NotNull(preActivationParticipant.CompletionCause);
        Assert.Null(session.HostConnectionSlot);
        Assert.Equal(1, sessions.HostSlotReleaseCalls);
        Assert.Equal(0, connections.ActiveConnectionCount);
    }

    [Fact]
    public async Task Final_Authorization_Rejection_Compensates_PreAccepted_Activation_Once()
    {
        var session = CreatePendingSession();
        var sessions = new FakeSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        var connections = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var validDeviceList = CreateDeviceList(
            session.OwnerUserId,
            HostDeviceId);
        using var services = CreateScopedServices(
            sessions,
            metrics,
            session.OwnerUserId,
            deviceListRepository:
                new RevokingFinalDeviceListRepository(validDeviceList));
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var handler = CreateHandler(
            connections,
            runtimes,
            broadcaster,
            scopeFactory,
            metrics,
            CreateMessageProcessor(
                runtimes,
                broadcaster,
                scopeFactory,
                metrics),
            new AlwaysValidJwtRevalidator(),
            NullLogger<HostWebSocketHandler>.Instance);
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "owner-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);

        await handler.HandleAsync(
            webSocket,
            session.Id.Value.ToString(),
            session.OwnerUserId,
            accessToken: null,
            CancellationToken.None);

        Assert.Equal(
            CloseReason.AccessRevoked.ToWire(),
            webSocket.LastCloseDescription);
        Assert.Null(runtimes.TryGet(session.Id));
        Assert.Null(session.HostConnectionSlot);
        Assert.Equal(SessionStatus.Reconnecting, session.Status);
        Assert.Equal(1, sessions.HostSlotReleaseCalls);
        Assert.Equal(0, connections.ActiveConnectionCount);
    }

    [Fact]
    public async Task Committed_Device_Revocation_Cancels_Host_While_Final_Authorization_Is_In_Flight()
    {
        var session = CreatePendingSession();
        var sessions = new FakeSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        var connections = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var deviceLists = new BlockingFinalDeviceListRepository(
            new FakeUserDeviceListRepository(
                CreateDeviceList(session.OwnerUserId, HostDeviceId)));
        using var services = CreateScopedServices(
            sessions,
            metrics,
            session.OwnerUserId,
            deviceListRepository: deviceLists);
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var handler = CreateHandler(
            connections,
            runtimes,
            broadcaster,
            scopeFactory,
            metrics,
            CreateMessageProcessor(
                runtimes,
                broadcaster,
                scopeFactory,
                metrics),
            new AlwaysValidJwtRevalidator(),
            NullLogger<HostWebSocketHandler>.Instance);
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "owner-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);

        var handling = handler.HandleAsync(
            webSocket,
            session.Id.Value.ToString(),
            session.OwnerUserId,
            accessToken: null,
            CancellationToken.None);
        await deviceLists.FinalReadStarted.WaitAsync(
            TestContext.Current.CancellationToken);

        Assert.Equal(1, connections.ActiveConnectionCount);
        Assert.Null(broadcaster.TryGetSession(session.Id)?.HostQueue);
        var effects = DeviceListRealtimeEffectsFactory.Create(
            connections,
            broadcaster,
            new UserEventBroadcaster(metrics, NullLogger<UserEventBroadcaster>.Instance),
            new FakeSessionEndAuthority(),
            new FakeDeviceRevocationSessionResolver(),
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);
        await effects.EnforceCommittedAsync(
            session.OwnerUserId,
            [HostDeviceId],
            [],
            TestContext.Current.CancellationToken);
        deviceLists.ReleaseFinalRead();
        await handling;

        Assert.Equal(
            CloseReason.AccessRevoked.ToWire(),
            webSocket.LastCloseDescription);
        Assert.Equal(0, connections.ActiveConnectionCount);
        Assert.Null(runtimes.TryGet(session.Id));
    }

    [Fact]
    public async Task HandleAsync_Does_Not_Publish_Runtime_When_Activation_Row_Disappears()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var session = CreatePendingSession();
        _ = broadcaster.GetOrCreateSession(session.Id);
        using var services = CreateScopedServices(
            new ValidationOnlySessionRepository(session),
            metrics,
            session.OwnerUserId);
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var messageProcessor = CreateMessageProcessor(
            runtimeDirectory,
            broadcaster,
            scopeFactory,
            metrics);
        var handler = CreateHandler(
            connectionRegistry,
            runtimeDirectory,
            broadcaster,
            scopeFactory,
            metrics,
            messageProcessor,
            new AlwaysValidJwtRevalidator(),
            NullLogger<HostWebSocketHandler>.Instance);
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "owner-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);

        await handler.HandleAsync(
            webSocket,
            session.Id.Value.ToString(),
            session.OwnerUserId,
            accessToken: null,
            CancellationToken.None);

        Assert.Equal(WebSocketCloseStatus.PolicyViolation, webSocket.LastCloseStatus);
        Assert.Equal("invalid_session_or_secret", webSocket.LastCloseDescription);
        Assert.Null(runtimeDirectory.TryGet(session.Id));
        Assert.Null(broadcaster.TryGetSession(session.Id));
    }

    [Fact]
    public async Task HandleAsync_Preserves_Waiting_Participants_When_PreAccept_Transition_Throws()
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var session = CreatePendingSession();
        var runtime = runtimeDirectory.CreateRuntime(session.Id);
        runtime.Participants.TryAddSharedParticipant("participant-1", int.MaxValue, out _);
        var sessionQueues = broadcaster.GetOrCreateSession(session.Id);
        sessionQueues.AddParticipantQueue("participant-1");
        using var services = CreateScopedServices(
            new ThrowOnTransitionSessionRepository(session),
            metrics,
            session.OwnerUserId);
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var messageProcessor = CreateMessageProcessor(
            runtimeDirectory,
            broadcaster,
            scopeFactory,
            metrics);
        var handler = CreateHandler(
            connectionRegistry,
            runtimeDirectory,
            broadcaster,
            scopeFactory,
            metrics,
            messageProcessor,
            new AlwaysValidJwtRevalidator(),
            NullLogger<HostWebSocketHandler>.Instance);
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "owner-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);

        await handler.HandleAsync(
            webSocket,
            session.Id.Value.ToString(),
            session.OwnerUserId,
            accessToken: null,
            CancellationToken.None);

        Assert.Equal(WebSocketCloseStatus.NormalClosure, webSocket.LastCloseStatus);
        Assert.Same(runtime, runtimeDirectory.TryGet(session.Id));
        Assert.Same(sessionQueues, broadcaster.TryGetSession(session.Id));
        Assert.Equal(1, runtime.Demand.GetStreamDemand().ParticipantCount);
    }

    [Theory]
    [InlineData(CommittedSessionEndReason.OwnerIdentityReset)]
    [InlineData(CommittedSessionEndReason.AccessRevoked)]
    public async Task Concurrent_Committed_End_Cannot_Be_Resurrected_By_Handshake_Finalization(
        CommittedSessionEndReason reason)
    {
        var runtimeDirectory = new LiveSessionStateDirectory();
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var lifecycleGate = new SessionLifecycleGate();
        var session = CreatePendingSession();
        var sessions = new FakeSessionRepository(session);
        var deviceLists = new BlockingFinalDeviceListRepository(
            new FakeUserDeviceListRepository(
                CreateDeviceList(session.OwnerUserId, HostDeviceId)));
        using var services = CreateScopedServices(
            sessions,
            metrics,
            session.OwnerUserId,
            deviceListRepository: deviceLists);
        var scopeFactory = services.GetRequiredService<IServiceScopeFactory>();
        var teardown = new HostTeardown(
            connectionRegistry,
            runtimeDirectory,
            broadcaster,
            scopeFactory,
            metrics,
            lifecycleGate,
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));
        var coordinator = new SessionEndCoordinator(
            scopeFactory,
            runtimeDirectory,
            broadcaster,
            teardown,
            lifecycleGate,
            NullLogger<SessionEndCoordinator>.Instance);
        var handshake = new HostHandshake(
            connectionRegistry,
            runtimeDirectory,
            broadcaster,
            new RealtimeDeviceAuthorizationReader(scopeFactory, TimeProvider.System),
            scopeFactory,
            metrics,
            lifecycleGate,
            NullLogger<HostHandshake>.Instance);
        var state = new HostConnectionState(
            "host-ending-during-finalization",
            session.Id.Value.ToString(),
            session.OwnerUserId);
        using var webSocket = new ScriptedWebSocket(
            [
                Frame.Text(
                    JsonSerializer.SerializeToUtf8Bytes(
                        new HostHelloMessage(
                            session.Id.Value.ToString(),
                            "owner-secret",
                            HostDeviceId,
                            ExpectedIncarnationId: session.IncarnationId,
                            RelayProtocolVersion: RelayProtocolVersions.Current),
                        WsJsonContext.Default.HostHelloMessage),
                    endOfMessage: true),
            ]);

        var accepting = handshake.TryAcceptAsync(
            webSocket,
            state,
            TestContext.Current.CancellationToken);
        await deviceLists.FinalReadStarted.WaitAsync(
            TestContext.Current.CancellationToken);

        var ending = EndCommittedAsync();
        async Task EndCommittedAsync()
        {
            await using var lifecycle = await coordinator.AcquireAsync(
                [session.Id],
                TestContext.Current.CancellationToken);
            session.End();
            await coordinator.ProjectCommittedAsync(
                [
                    new CommittedSessionEnd(
                        new LiveSessionTransitionResult(
                            SessionTransitionOutcome.Applied,
                            session.ToDiscoveryTarget()),
                        reason),
                ],
                TestContext.Current.CancellationToken);
        }
        await WaitForReferenceCountAsync(lifecycleGate, session.Id, 2);
        deviceLists.ReleaseFinalRead();

        Assert.True(await accepting);
        await ending;
        await teardown.RunAsync(
            webSocket,
            state,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionStatus.Ended, session.Status);
        Assert.True(state.HostAccepted);
        Assert.NotNull(state.HostQueue);
        Assert.Null(runtimeDirectory.TryGet(session.Id));
        Assert.Null(broadcaster.TryGetSession(session.Id));
        Assert.Equal(0, lifecycleGate.TestActiveSessionCount());
    }

    private static ServiceProvider CreateScopedServices(
        ISessionRepository sessions,
        OperationalMetrics metrics,
        UserId hostUserId,
        IUserDeviceListRepository? deviceListRepository = null,
        ScopedLifetimeProbe? scopedLifetime = null,
        ICollection<KodosiDbContext>? dbContexts = null,
        IUnitOfWork? unitOfWork = null)
    {
        scopedLifetime ??= new ScopedLifetimeProbe();
        return new ServiceCollection()
            .AddScoped(_ => scopedLifetime.CreateLease())
            .AddScoped(_ =>
            {
                var options = new DbContextOptionsBuilder<KodosiDbContext>()
                    .UseNpgsql("Host=localhost;Database=lifetime_only")
                    .Options;
                var dbContext = new KodosiDbContext(options);
                dbContexts?.Add(dbContext);
                return dbContext;
            })
            .AddScoped<ISessionRepository>(services =>
            {
                _ = services.GetRequiredService<ScopedLifetimeLease>();
                if (dbContexts is not null)
                {
                    _ = services.GetRequiredService<KodosiDbContext>();
                }
                return sessions;
            })
            .AddScoped<IUserDeviceRepository>(services =>
            {
                _ = services.GetRequiredService<ScopedLifetimeLease>();
                if (dbContexts is not null)
                {
                    _ = services.GetRequiredService<KodosiDbContext>();
                }
                return new FakeUserDeviceRepository(
                    CreateCertifiedDevice(hostUserId, HostDeviceId));
            })
            .AddScoped<IUserDeviceListRepository>(services =>
            {
                _ = services.GetRequiredService<ScopedLifetimeLease>();
                if (dbContexts is not null)
                {
                    _ = services.GetRequiredService<KodosiDbContext>();
                }
                return deviceListRepository
                    ?? new FakeUserDeviceListRepository(
                        CreateDeviceList(hostUserId, HostDeviceId));
            })
            .AddScoped<ISessionKeyBlobRepository>(_ => new FakeSessionKeyBlobRepository())
            .AddScoped<ISessionEndMutationRepository>(
                _ => new FakeSessionEndMutationRepository())
            .AddScoped<IPopSignatureVerifier>(_ => new DeterministicPopVerifier())
            .AddScoped<IOwnerSessionSecretHasher, OwnerSessionSecretHasher>()
            .AddScoped<IUnitOfWork>(_ => unitOfWork ?? new NoOpUnitOfWork())
            .AddScoped<IUserLifecycleLock>(_ => new FakeUserLifecycleLock())
            .AddScoped<IFriendshipRepository, EmptyFriendshipRepository>()
            .AddScoped<IRoomMemberRepository, EmptyRoomMemberRepository>()
            .AddScoped<ISessionViewerDismissalRepository>(
                _ => new FakeSessionViewerDismissalRepository())
            .AddScoped<UserEventBroadcaster>(_ => new UserEventBroadcaster(metrics, NullLogger<UserEventBroadcaster>.Instance))
            .AddScoped<DiscoveryAudienceResolver>()
            .AddScoped<SharedSurfaceEventPublisher>()
            .AddScoped<HostSessionActivator>()
            .AddScoped<LiveSessionTransitionOrchestrator>()
            .AddScoped<LiveSessionStatusReader>()
            .AddScoped<LiveSessionTerminator>()
            .BuildServiceProvider();
    }

    private static UserDevice CreateCertifiedDevice(UserId userId, string deviceId)
    {
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Test host",
            signerDeviceId: deviceId,
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: null);
        return device;
    }

    private static UserDeviceList CreateDeviceList(UserId userId, string deviceId) =>
        TestDeviceList.Create(
            userId,
            1,
            $$"""[{"deviceId":"{{deviceId}}","signerDeviceId":"{{deviceId}}"}]""",
            deviceId,
            [2],
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            null);

    private static HostWebSocketHandler CreateHandler(
        IConnectionRegistry connectionRegistry,
        ILiveSessionStateDirectory runtimeDirectory,
        SessionBroadcaster broadcaster,
        IServiceScopeFactory scopeFactory,
        OperationalMetrics metrics,
        HostMessageProcessor messageProcessor,
        IJwtRevalidator jwtRevalidator,
        ILogger<HostWebSocketHandler> logger)
    {
        var lifecycleGate = new SessionLifecycleGate();
        return new HostWebSocketHandler(
            new HostHandshake(
                connectionRegistry,
                runtimeDirectory,
                broadcaster,
                new RealtimeDeviceAuthorizationReader(scopeFactory, TimeProvider.System),
                scopeFactory,
                metrics,
                lifecycleGate,
                NullLogger<HostHandshake>.Instance),
            new HostSessionPump(
                broadcaster,
                messageProcessor,
                jwtRevalidator,
                metrics,
                NullLogger<HostSessionPump>.Instance),
            new HostTeardown(
                connectionRegistry,
                runtimeDirectory,
                broadcaster,
                scopeFactory,
                metrics,
                lifecycleGate,
                NullLogger<HostTeardown>.Instance,
                repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System)),
            logger,
            TimeProvider.System);
    }

    private static async Task WaitForReferenceCountAsync(
        SessionLifecycleGate gate,
        SessionId sessionId,
        int expected)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        while (gate.TestReferenceCount(sessionId) < expected)
        {
            await Task.Delay(1, timeout.Token);
        }
    }

    private static HostMessageProcessor CreateMessageProcessor(
        ILiveSessionStateDirectory runtimeDirectory,
        SessionBroadcaster broadcaster,
        IServiceScopeFactory scopeFactory,
        OperationalMetrics metrics)
    {
        var lifecycleGate = new SessionLifecycleGate();
        var connections = new ConnectionRegistry();
        var teardown = new HostTeardown(
            connections,
            runtimeDirectory,
            broadcaster,
            scopeFactory,
            metrics,
            lifecycleGate,
            NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));
        var sessionEndCoordinator = new SessionEndCoordinator(
            scopeFactory,
            runtimeDirectory,
            broadcaster,
            teardown,
            lifecycleGate,
            NullLogger<SessionEndCoordinator>.Instance);
        return new HostMessageProcessor(
            broadcaster,
            sessionEndCoordinator,
            lifecycleGate,
            runtimeDirectory,
            metrics,
            NullLogger<HostMessageProcessor>.Instance,
            UnsupportedSemanticRelayRepository.Instance,
            connections,
            new ActionDedupeCache(),
            UnsupportedPermissionDecisionAuditStore.Instance,
            scopeFactory);
    }

    private static Session CreatePendingSession(
        string secret = "owner-secret",
        bool currentIncarnation = true)
    {
        return Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            incarnationGeneration: 1,
            currentIncarnation
                ? Session.CurrentIncarnationProtocolVersion
                : Session.LegacyIncarnationProtocolVersion,
            UserId.New(),
            "Pending Session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            new OwnerSessionSecretHasher().Hash(secret));
    }

    private static void SetHostSlot(Session session, string connectionId)
    {
        typeof(Session)
            .GetProperty(nameof(Session.HostConnectionSlot))!
            .SetValue(session, connectionId);
    }

    private sealed class BlockingFinalDeviceListRepository(
        IUserDeviceListRepository inner) : IUserDeviceListRepository
    {
        private readonly IUserDeviceListRepository _inner = inner;
        private readonly TaskCompletionSource _finalReadStarted =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        private readonly TaskCompletionSource _releaseFinalRead =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        private int _readCount;

        public Task FinalReadStarted => _finalReadStarted.Task;

        public void ReleaseFinalRead() => _releaseFinalRead.TrySetResult();

        public async Task<UserDeviceList?> GetLatestAsync(
            UserId userId,
            CancellationToken ct = default)
        {
            if (Interlocked.Increment(ref _readCount) == 2)
            {
                _finalReadStarted.TrySetResult();
                await _releaseFinalRead.Task.WaitAsync(ct);
            }
            return await _inner.GetLatestAsync(userId, ct);
        }

        public Task<UserDeviceList?> GetGenerationAsync(
            UserId userId,
            long generation,
            CancellationToken ct = default) =>
            _inner.GetGenerationAsync(userId, generation, ct);

        public Task<IReadOnlyList<UserDeviceList>> GetLatestByUserIdsAsync(
            IReadOnlyCollection<UserId> userIds,
            CancellationToken ct = default) =>
            _inner.GetLatestByUserIdsAsync(userIds, ct);

        public Task AddAsync(UserDeviceList list, CancellationToken ct = default) =>
            _inner.AddAsync(list, ct);

        public Task<int> RemoveAllForUserAsync(UserId userId, CancellationToken ct = default) =>
            _inner.RemoveAllForUserAsync(userId, ct);
    }

    private sealed class RevokingFinalDeviceListRepository(
        UserDeviceList validList) : IUserDeviceListRepository
    {
        private int _readCount;

        public Task<UserDeviceList?> GetLatestAsync(
            UserId userId,
            CancellationToken ct = default) =>
            Task.FromResult<UserDeviceList?>(
                Interlocked.Increment(ref _readCount) == 1
                    ? validList
                    : null);

        public Task<UserDeviceList?> GetGenerationAsync(
            UserId userId,
            long generation,
            CancellationToken ct = default) =>
            Task.FromResult<UserDeviceList?>(
                validList.UserId == userId && validList.Generation == generation
                    ? validList
                    : null);

        public Task<IReadOnlyList<UserDeviceList>> GetLatestByUserIdsAsync(
            IReadOnlyCollection<UserId> userIds,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<UserDeviceList>>([]);

        public Task AddAsync(
            UserDeviceList list,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task<int> RemoveAllForUserAsync(
            UserId userId,
            CancellationToken ct = default) =>
            Task.FromResult(0);
    }

    private sealed class ValidationOnlySessionRepository(Session session)
        : SessionRepositoryStub
    {
        private readonly Session _session = session;
        private int _getByIdCalls;

        public override Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)
        {
            if (id != _session.Id)
            {
                return Task.FromResult<Session?>(null);
            }

            _getByIdCalls++;
            return Task.FromResult(_getByIdCalls == 1 ? _session : null);
        }

        public override Task<Session?> GetByIdForUpdateAsync(
            SessionId id,
            CancellationToken ct = default) =>
            Task.FromResult<Session?>(null);

        public override Task AddAsync(Session session, CancellationToken ct = default) => Task.CompletedTask;

        public override Task UpdateAsync(Session session, CancellationToken ct = default)
        {
            return Task.CompletedTask;
        }

        public override Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(
            UserId ownerUserId,
            CancellationToken ct = default)
            => throw new NotSupportedException();

        public override Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
            RoomId roomId,
            FeedCursor? cursor = null,
            int limit = 20,
            ToolKind? toolKindFilter = null,
            DateTimeOffset? since = null,
            CancellationToken ct = default)
            => throw new NotSupportedException();

        public override Task ReleaseHostSlotAsync(
            SessionId sessionId,
            string? connectionId,
            CancellationToken ct = default) =>
            Task.CompletedTask;

    }

    private sealed class ThrowOnTransitionSessionRepository(Session session)
        : SessionRepositoryStub
    {
        private readonly Session _session = session;
        private int _getByIdCalls;

        public override Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)
        {
            if (id != _session.Id)
            {
                return Task.FromResult<Session?>(null);
            }

            _getByIdCalls++;
            if (_getByIdCalls == 1)
            {
                return Task.FromResult<Session?>(_session);
            }

            throw new InvalidOperationException("Simulated transition failure");
        }

        public override Task AddAsync(Session session, CancellationToken ct = default) => Task.CompletedTask;

        public override Task UpdateAsync(Session session, CancellationToken ct = default)
        {
            return Task.CompletedTask;
        }

        public override Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(
            UserId ownerUserId,
            CancellationToken ct = default)
            => throw new NotSupportedException();

        public override Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
            RoomId roomId,
            FeedCursor? cursor = null,
            int limit = 20,
            ToolKind? toolKindFilter = null,
            DateTimeOffset? since = null,
            CancellationToken ct = default)
            => throw new NotSupportedException();

    }

    private sealed class DbSlotClaimRejectingRepository(Session session)
        : SessionRepositoryStub
    {
        private readonly Session _session = session;

        public override Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)
            => Task.FromResult(id == _session.Id ? _session : null);
        public override Task<Session?> GetByIdForUpdateAsync(
            SessionId id,
            CancellationToken ct = default) =>
            Task.FromResult(id == _session.Id ? _session : null);

        public override Task AddAsync(Session session, CancellationToken ct = default) => Task.CompletedTask;
        public override Task UpdateAsync(Session session, CancellationToken ct = default) => Task.CompletedTask;

        public override Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(
            UserId ownerUserId, CancellationToken ct = default) => throw new NotSupportedException();
        public override Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
            RoomId roomId, FeedCursor? cursor = null, int limit = 20,
            ToolKind? toolKindFilter = null, DateTimeOffset? since = null,
            CancellationToken ct = default) => throw new NotSupportedException();
    }

    private sealed class NoOpUnitOfWork : UnitOfWorkStub
    {
        public override Task SaveChangesAsync(CancellationToken ct = default) => Task.CompletedTask;

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default) =>
            Task.FromResult<ITransactionScope>(new CompletedTransactionScope());
    }

    private sealed class BlockingNullObservationRuntimeDirectory :
        ILiveSessionStateDirectory,
        IDisposable
    {
        private readonly LiveSessionStateDirectory _inner = new();
        private readonly ManualResetEventSlim _releaseNullObservation = new(false);
        private readonly TaskCompletionSource _nullObservationStarted =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        private int _blockNextNullObservation = 1;

        public Task NullObservationStarted => _nullObservationStarted.Task;

        public LiveSessionPorts? TryGet(SessionId sessionId)
        {
            var observed = _inner.TryGet(sessionId);
            if (observed is null
                && Interlocked.CompareExchange(
                    ref _blockNextNullObservation,
                    0,
                    1) == 1)
            {
                _nullObservationStarted.TrySetResult();
                _releaseNullObservation.Wait();
            }

            return observed;
        }

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

        public bool RemoveIfSame(
            SessionId sessionId,
            LiveSessionPorts expected) =>
            _inner.RemoveIfSame(sessionId, expected);

        public bool RemoveIfOwned(
            SessionId sessionId,
            LiveSessionCreationOwnership creationOwnership) =>
            _inner.RemoveIfOwned(sessionId, creationOwnership);

        public IReadOnlyList<SessionId> GetActiveSessions() =>
            _inner.GetActiveSessions();


        public void ReleaseNullObservation() =>
            _releaseNullObservation.Set();

        public void Dispose() =>
            _releaseNullObservation.Dispose();
    }

    public enum HostActivationFailurePoint
    {
        Save,
        Commit,
    }

    private sealed class FailingOnceUnitOfWork(
        HostActivationFailurePoint failurePoint) : UnitOfWorkStub
    {
        private readonly HostActivationFailurePoint _failurePoint =
            failurePoint;
        private int _failed;

        public override Task SaveChangesAsync(CancellationToken ct = default)
        {
            if (_failurePoint == HostActivationFailurePoint.Save
                && Interlocked.Exchange(ref _failed, 1) == 0)
            {
                throw new InvalidOperationException(
                    "Injected host activation save failure.");
            }

            return Task.CompletedTask;
        }

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default) =>
            Task.FromResult<ITransactionScope>(
                new FailingOnceTransaction(this));

        private sealed class FailingOnceTransaction(
            FailingOnceUnitOfWork owner) : TransactionScopeStub
        {
            public override Task CommitAsync(CancellationToken ct = default)
            {
                if (owner._failurePoint == HostActivationFailurePoint.Commit
                    && Interlocked.Exchange(ref owner._failed, 1) == 0)
                {
                    throw new InvalidOperationException(
                        "Injected host activation commit failure.");
                }

                return Task.CompletedTask;
            }

            public override ValueTask DisposeAsync() => ValueTask.CompletedTask;
        }
    }

    private sealed class AlwaysValidJwtRevalidator : IJwtRevalidator
    {
        public Task<JwtRevalidationResult> RevalidateAsync(string token, CancellationToken ct)
            => Task.FromResult(JwtRevalidationResult.Valid);
    }

    private sealed class EmptyFriendshipRepository : IFriendshipRepository
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

        public Task AddAsync(Friendship friendship, CancellationToken ct = default) => Task.CompletedTask;

        public void Remove(Friendship friendship) { }
    }

    private sealed class EmptyRoomMemberRepository : IRoomMemberRepository
    {
        public Task<bool> IsMemberAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
            => Task.FromResult(false);

        public Task<IReadOnlyList<RoomId>> GetActiveRoomIdsForUserAsync(
            UserId userId,
            IReadOnlyList<RoomId> roomIds,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<RoomId>>([]);

        public Task<RoomMember?> GetAsync(RoomId roomId, UserId userId, CancellationToken ct = default)
            => Task.FromResult<RoomMember?>(null);

        public Task AddAsync(RoomMember member, CancellationToken ct = default) => Task.CompletedTask;

        public Task<IReadOnlyList<UserId>> GetMemberUserIdsAsync(RoomId roomId, CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<UserId>>([]);

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

    private sealed class DeterministicPopVerifier : IPopSignatureVerifier
    {
        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature) =>
            !publicKey.IsEmpty && !message.IsEmpty && signature.SequenceEqual(new byte[] { 1 });
    }

    private sealed record Frame(WebSocketMessageType MessageType, byte[] Payload, bool EndOfMessage)
    {
        public static Frame Text(byte[] payload, bool endOfMessage = false)
            => new(WebSocketMessageType.Text, payload, endOfMessage);
    }

    private sealed class ScriptedWebSocket(
        IReadOnlyList<Frame> frames,
        bool blockCloseOutput = false,
        string proofType = "device.proof",
        string? proofSessionId = null) : WebSocket
    {
        private readonly Queue<Frame> _frames = new(PrependProof(
            frames,
            proofType,
            proofSessionId));
        private WebSocketState _state = WebSocketState.Open;


        private static IReadOnlyList<Frame> PrependProof(
            IReadOnlyList<Frame> frames,
            string proofType,
            string? proofSessionId)
        {
            var helloBytes = frames.SelectMany(frame => frame.Payload).ToArray();
            Guid? incarnation = null;
            string? sessionId = null;
            try
            {
                using var hello = JsonDocument.Parse(helloBytes);
                sessionId = hello.RootElement.TryGetProperty("sessionId", out var sid)
                    ? sid.GetString()
                    : null;
                incarnation = hello.RootElement.TryGetProperty("expectedIncarnationId", out var inc)
                    && inc.ValueKind == JsonValueKind.String
                    && Guid.TryParse(inc.GetString(), out var parsed)
                    ? parsed
                    : null;
            }
            catch (JsonException)
            {
            }
            var proof = JsonSerializer.SerializeToUtf8Bytes(new DeviceProofResponseMessage(
                proofType,
                HostDeviceId,
                proofSessionId ?? sessionId,
                incarnation,
                Convert.ToBase64String(new byte[] { 1 })));
            return [Frame.Text(proof, endOfMessage: true), .. frames];
        }

        public override WebSocketCloseStatus? CloseStatus => LastCloseStatus;
        public override string? CloseStatusDescription => LastCloseDescription;
        public override WebSocketState State => _state;
        public override string? SubProtocol => null;
        public WebSocketCloseStatus? LastCloseStatus { get; private set; }
        public string? LastCloseDescription { get; private set; }
        public TaskCompletionSource CloseOutputStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public override void Abort()
        {
            _state = WebSocketState.Aborted;
        }

        public override Task CloseAsync(WebSocketCloseStatus closeStatus, string? statusDescription, CancellationToken cancellationToken)
        {
            LastCloseStatus = closeStatus;
            LastCloseDescription = statusDescription;
            _state = WebSocketState.Closed;
            return Task.CompletedTask;
        }

        public override Task CloseOutputAsync(WebSocketCloseStatus closeStatus, string? statusDescription, CancellationToken cancellationToken)
        {
            LastCloseStatus = closeStatus;
            LastCloseDescription = statusDescription;
            CloseOutputStarted.TrySetResult();
            if (blockCloseOutput)
            {
                return new TaskCompletionSource(
                    TaskCreationOptions.RunContinuationsAsynchronously).Task;
            }
            _state = WebSocketState.CloseSent;
            return Task.CompletedTask;
        }

        public override void Dispose()
        {
            _state = WebSocketState.Closed;
        }

        public override Task<WebSocketReceiveResult> ReceiveAsync(ArraySegment<byte> buffer, CancellationToken cancellationToken)
        {
            if (_frames.Count == 0)
            {
                _state = WebSocketState.CloseReceived;
                return Task.FromResult(new WebSocketReceiveResult(0, WebSocketMessageType.Close, true));
            }

            var frame = _frames.Dequeue();
            if (frame.Payload.Length > buffer.Count)
            {
                throw new InvalidOperationException("Test frame payload exceeds receive buffer.");
            }

            frame.Payload.AsSpan().CopyTo(buffer.AsSpan());
            return Task.FromResult(
                new WebSocketReceiveResult(frame.Payload.Length, frame.MessageType, frame.EndOfMessage));
        }

        public override Task SendAsync(ArraySegment<byte> buffer, WebSocketMessageType messageType, bool endOfMessage, CancellationToken cancellationToken)
            => Task.CompletedTask;
    }
}
