using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;
using System.Collections.Concurrent;
using System.Net.WebSockets;
using System.Reflection;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Host.Serialization;
using Kodosi.Infrastructure.Realtime;
using Kodosi.Infrastructure.Crypto;

namespace Kodosi.HostTests;

public sealed class HeartbeatTimeoutServiceTests
{
    [Fact]
    public async Task SweepAsync_Repairs_Failed_HostSlot_And_ParticipantCount_Writes()
    {
        var repository = new RepairSessionRepository();
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimes = new LiveSessionStateDirectory(clock);
        var metrics = new OperationalMetrics(runtimes);
        var tracker = new RealtimePersistenceRepairTracker(TimeProvider.System);
        var sessionId = SessionId.New();
        var ports = runtimes.CreateRuntime(sessionId);
        var startedAt = DateTimeOffset.UtcNow;
        ports.Host.SetSessionStartedAt(startedAt);
        ports.Participants.TryAddSharedParticipant("viewer", 50, out _);
        tracker.MarkHostRelease(sessionId, "host");
        tracker.MarkParticipantCount(
            sessionId,
            startedAt,
            ports.IncarnationId);
        using var services = CreateScopedServices(repository, metrics);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimes,
            new ConnectionRegistry(),
            broadcaster,
            metrics,
            clock,
            tracker);

        await service.SweepAsync(TestContext.Current.CancellationToken);

        Assert.Equal([(sessionId, "host")], repository.HostReleases);
        Assert.Equal([(sessionId, 1)], repository.ParticipantCounts);
        Assert.Empty(tracker.Snapshot().HostReleases);
        Assert.Empty(tracker.Snapshot().ParticipantCounts);
    }

    [Fact]
    public async Task ParticipantCountRepair_Holds_Lifecycle_Through_Conditional_Write()
    {
        var repository = new BlockingRepairSessionRepository();
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimes = new LiveSessionStateDirectory(clock);
        var metrics = new OperationalMetrics(runtimes);
        var tracker = new RealtimePersistenceRepairTracker(TimeProvider.System);
        var gate = new SessionLifecycleGate();
        var sessionId = SessionId.New();
        var runtime = runtimes.CreateRuntime(sessionId);
        var startedAt = clock.GetUtcNow();
        runtime.Host.SetSessionStartedAt(startedAt);
        runtime.Participants.TryAddSharedParticipant("viewer", 50, out _);
        tracker.MarkParticipantCount(
            sessionId,
            startedAt,
            runtime.IncarnationId);
        using var services = CreateScopedServices(repository, metrics);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimes,
            new ConnectionRegistry(),
            broadcaster,
            metrics,
            clock,
            tracker,
            gate);

        var repairing = service.SweepAsync(
            TestContext.Current.CancellationToken);
        await repository.WriteStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        var joining = gate.AcquireAsync(
            sessionId,
            TestContext.Current.CancellationToken).AsTask();
        await Task.Delay(25, TestContext.Current.CancellationToken);
        Assert.False(joining.IsCompleted);

        repository.Release();
        await repairing;
        await using var joinLease = await joining;
        runtime.Participants.TryAddSharedParticipant("viewer-2", 50, out _);

        Assert.Equal([(sessionId, startedAt, 1)], repository.Writes);
        Assert.Equal(
            2,
            runtime.Demand.GetStreamDemand().SharedParticipantCount);
    }

    [Fact]
    public async Task SweepStaleSharedParticipants_Removes_Stale_Connections_From_Registry()
    {
        var services = new ServiceCollection().BuildServiceProvider();
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connectionRegistry,
            broadcaster,
            metrics,
            clock);
        var sessionId = SessionId.New();
        var userId = UserId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Participants.TryAddSharedParticipant("viewer-1", int.MaxValue, out _);
        connectionRegistry.RegisterSharedParticipant("viewer-1", userId, "device-1", sessionId);
        var sessionQueues = broadcaster.GetOrCreateSession(sessionId);
        sessionQueues.AddParticipantQueue("viewer-1");
        var hostQueue = sessionQueues.SetHostQueue();
        clock.Advance(TimeSpan.FromMinutes(2));

        await service.SweepStaleSharedParticipants(
            sessionId,
            runtime.Participants,
            runtime.Demand,
            TimeSpan.FromMinutes(1),
            TestContext.Current.CancellationToken);

        Assert.Empty(connectionRegistry.GetActiveSharedParticipants(sessionId));
        Assert.Equal(0, runtime.Demand.GetStreamDemand().SharedParticipantCount);

        var firstHostMessage = await hostQueue.ReadAsync(CancellationToken.None);
        var secondHostMessage = await hostQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(firstHostMessage);
        Assert.NotNull(secondHostMessage);
        Assert.Contains("\"required\":false", System.Text.Encoding.UTF8.GetString(firstHostMessage!));
        Assert.Contains("\"action\":\"left\"", System.Text.Encoding.UTF8.GetString(secondHostMessage!));
    }

    [Fact]
    public async Task SweepStaleSharedParticipants_Decrements_Db_Count_For_Every_Stale_Connection()
    {
        var countingRepo = new CountingSessionRepository();
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var metrics = new OperationalMetrics(runtimeDirectory);
        var connectionRegistry = new ConnectionRegistry();
        var services = CreateScopedServices(countingRepo, metrics);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connectionRegistry,
            broadcaster,
            metrics,
            clock);

        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.SetSessionStartedAt(clock.GetUtcNow());
        var sessionQueues = broadcaster.GetOrCreateSession(sessionId);
        sessionQueues.SetHostQueue();

        for (var i = 0; i < 5; i++)
        {
            var connectionId = $"viewer-{i}";
            runtime.Participants.TryAddSharedParticipant(connectionId, int.MaxValue, out _);
            connectionRegistry.RegisterSharedParticipant(connectionId, UserId.New(), $"device-{i}", sessionId);
            sessionQueues.AddParticipantQueue(connectionId);
        }
        clock.Advance(TimeSpan.FromMinutes(2));

        await service.SweepStaleSharedParticipants(
            sessionId,
            runtime.Participants,
            runtime.Demand,
            TimeSpan.FromMinutes(1),
            TestContext.Current.CancellationToken);

        Assert.Equal(5, countingRepo.DecrementCalls);
        Assert.Empty(connectionRegistry.GetActiveSharedParticipants(sessionId));
        Assert.Equal(0, runtime.Demand.GetStreamDemand().SharedParticipantCount);
    }

    [Fact]
    public async Task SweepStaleSharedParticipants_Continues_After_One_Thrown_Decrement()
    {
        var countingRepo = new CountingSessionRepository(throwOnCall: 3);
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var metrics = new OperationalMetrics(runtimeDirectory);
        var tracker = new RealtimePersistenceRepairTracker(TimeProvider.System);
        var connectionRegistry = new ConnectionRegistry();
        var services = CreateScopedServices(countingRepo, metrics);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connectionRegistry,
            broadcaster,
            metrics,
            clock,
            tracker);

        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Host.SetSessionStartedAt(clock.GetUtcNow());
        var sessionQueues = broadcaster.GetOrCreateSession(sessionId);
        sessionQueues.SetHostQueue();

        for (var i = 0; i < 5; i++)
        {
            var connectionId = $"viewer-{i}";
            runtime.Participants.TryAddSharedParticipant(connectionId, int.MaxValue, out _);
            connectionRegistry.RegisterSharedParticipant(connectionId, UserId.New(), $"device-{i}", sessionId);
            sessionQueues.AddParticipantQueue(connectionId);
        }
        clock.Advance(TimeSpan.FromMinutes(2));

        await service.SweepStaleSharedParticipants(
            sessionId,
            runtime.Participants,
            runtime.Demand,
            TimeSpan.FromMinutes(1),
            TestContext.Current.CancellationToken);

        Assert.Equal(5, countingRepo.DecrementCalls);
        Assert.Empty(connectionRegistry.GetActiveSharedParticipants(sessionId));
        Assert.Equal(0, runtime.Demand.GetStreamDemand().SharedParticipantCount);
        Assert.Equal(
            [sessionId],
            tracker.Snapshot().ParticipantCounts.Select(repair => repair.SessionId));
        Assert.Equal(1, metrics.Snapshot().ParticipantDecrementFailureCount);

        Assert.True(runtimeDirectory.RemoveIfSame(sessionId, runtime));
        await service.SweepAsync(TestContext.Current.CancellationToken);

        Assert.Empty(countingRepo.ParticipantCounts);
        Assert.Equal([sessionId], countingRepo.ParticipantCountClears);
        Assert.Empty(tracker.Snapshot().ParticipantCounts);
        Assert.Equal(1, metrics.Snapshot().ParticipantDecrementFailureCount);
    }

    [Fact]
    public async Task ParticipantTeardown_Failure_Clears_Persisted_Count_After_Runtime_Removal()
    {
        var repository = new CountingSessionRepository(throwOnCall: 1);
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimes = new LiveSessionStateDirectory(clock);
        var metrics = new OperationalMetrics(runtimes);
        var tracker = new RealtimePersistenceRepairTracker(TimeProvider.System);
        var lifecycleGate = new SessionLifecycleGate();
        var sessionId = SessionId.New();
        var startedAt = clock.GetUtcNow();
        var runtime = runtimes.CreateRuntime(sessionId);
        runtime.Host.SetSessionStartedAt(startedAt);
        runtime.Participants.TryAddSharedParticipant("viewer", 50, out _);
        var connections = new ConnectionRegistry();
        var viewerId = UserId.New();
        connections.RegisterSharedParticipant(
            "viewer",
            viewerId,
            "viewer-device",
            sessionId);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var participantQueue = queues.AddParticipantQueue("viewer");
        using var services = CreateScopedServices(repository, metrics);
        var state = new ParticipantConnectionState(
            "viewer",
            sessionId.Value.ToString(),
            viewerId)
        {
            SessionId = sessionId,
            Ports = runtime,
            SessionQueues = queues,
            ParticipantQueue = participantQueue,
            ParticipantRegistered = true,
            DbParticipantCounted = true,
            AccessDecision = new ParticipantAccessDecision(
                IsOwnerParticipant: false,
                AccessLevel.View,
                startedAt),
        };
        var teardown = new ParticipantTeardown(
            connections,
            runtimes,
            services.GetRequiredService<IServiceScopeFactory>(),
            broadcaster,
            metrics,
            lifecycleGate,
            NullLogger<ParticipantTeardown>.Instance,
            tracker);

        await teardown.RunAsync(
            new ClosedWebSocket(),
            state,
            TestContext.Current.CancellationToken);
        Assert.Single(tracker.Snapshot().ParticipantCounts);
        Assert.True(runtimes.RemoveIfSame(sessionId, runtime));

        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimes,
            connections,
            broadcaster,
            metrics,
            clock,
            tracker,
            lifecycleGate);
        await service.SweepAsync(TestContext.Current.CancellationToken);

        Assert.Equal([sessionId], repository.ParticipantCountClears);
        Assert.Empty(tracker.Snapshot().ParticipantCounts);
    }

    [Fact]
    public async Task ParticipantCountRepair_Does_Not_Overwrite_Replacement_Runtime_For_Same_StartedAt()
    {
        var repository = new RepairSessionRepository();
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimes = new LiveSessionStateDirectory(clock);
        var metrics = new OperationalMetrics(runtimes);
        var tracker = new RealtimePersistenceRepairTracker(TimeProvider.System);
        var sessionId = SessionId.New();
        var startedAt = clock.GetUtcNow();
        var staleRuntime = runtimes.CreateRuntime(sessionId);
        staleRuntime.Host.SetSessionStartedAt(startedAt);
        tracker.MarkParticipantCount(
            sessionId,
            startedAt,
            staleRuntime.IncarnationId);
        Assert.True(runtimes.RemoveIfSame(sessionId, staleRuntime));
        var replacementRuntime = runtimes.CreateRuntime(sessionId);
        replacementRuntime.Host.SetSessionStartedAt(startedAt);
        replacementRuntime.Participants.TryAddSharedParticipant(
            "viewer-1",
            50,
            out _);
        replacementRuntime.Participants.TryAddSharedParticipant(
            "viewer-2",
            50,
            out _);
        using var services = CreateScopedServices(repository, metrics);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimes,
            new ConnectionRegistry(),
            broadcaster,
            metrics,
            clock,
            tracker);

        await service.SweepAsync(TestContext.Current.CancellationToken);

        Assert.Empty(repository.ParticipantCounts);
        Assert.Empty(tracker.Snapshot().ParticipantCounts);
    }

    [Fact]
    public async Task ParticipantCountRepair_Does_Not_Reduce_Republished_Incarnation()
    {
        var repository = new RepairSessionRepository();
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimes = new LiveSessionStateDirectory(clock);
        var metrics = new OperationalMetrics(runtimes);
        var tracker = new RealtimePersistenceRepairTracker(TimeProvider.System);
        var sessionId = SessionId.New();
        var oldStartedAt = clock.GetUtcNow();
        var staleRuntime = runtimes.CreateRuntime(sessionId);
        staleRuntime.Host.SetSessionStartedAt(oldStartedAt);
        tracker.MarkParticipantCount(
            sessionId,
            oldStartedAt,
            staleRuntime.IncarnationId);
        Assert.True(runtimes.RemoveIfSame(sessionId, staleRuntime));
        clock.Advance(TimeSpan.FromMilliseconds(1));
        var replacementRuntime = runtimes.CreateRuntime(sessionId);
        replacementRuntime.Host.SetSessionStartedAt(clock.GetUtcNow());
        replacementRuntime.Participants.TryAddSharedParticipant(
            "replacement-viewer",
            50,
            out _);
        using var services = CreateScopedServices(repository, metrics);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimes,
            new ConnectionRegistry(),
            broadcaster,
            metrics,
            clock,
            tracker);

        await service.SweepAsync(TestContext.Current.CancellationToken);

        Assert.Empty(repository.ParticipantCounts);
        Assert.Empty(repository.ParticipantCountClears);
        Assert.Equal(
            1,
            replacementRuntime.Demand.GetStreamDemand().SharedParticipantCount);
        Assert.Empty(tracker.Snapshot().ParticipantCounts);
    }

    [Fact]
    public async Task SweepStaleParticipant_ActivityRefresh_Wins_While_Waiting_For_Lifecycle()
    {
        var repository = new CountingSessionRepository();
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimes = new LiveSessionStateDirectory(clock);
        var connections = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        using var services = CreateScopedServices(repository, metrics);
        var lifecycleGate = new SessionLifecycleGate();
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimes,
            connections,
            broadcaster,
            metrics,
            clock,
            lifecycleGate: lifecycleGate);
        var sessionId = SessionId.New();
        var runtime = runtimes.CreateRuntime(sessionId);
        runtime.Host.SetSessionStartedAt(clock.GetUtcNow());
        runtime.Participants.TryAddSharedParticipant("viewer", 50, out _);
        connections.RegisterSharedParticipant(
            "viewer",
            UserId.New(),
            "device",
            sessionId);
        var queues = broadcaster.GetOrCreateSession(sessionId);
        var queue = queues.AddParticipantQueue("viewer");
        clock.Advance(TimeSpan.FromMinutes(2));

        var gateLease = await lifecycleGate.AcquireAsync(
            sessionId,
            TestContext.Current.CancellationToken);
        var sweeping = service.SweepStaleSharedParticipants(
            sessionId,
            runtime.Participants,
            runtime.Demand,
            TimeSpan.FromMinutes(1),
            TestContext.Current.CancellationToken);
        await WaitForReferenceCountAsync(lifecycleGate, sessionId, 2);
        runtime.Participants.RecordParticipantActivity("viewer");
        await gateLease.DisposeAsync();
        await sweeping;

        Assert.Single(connections.GetActiveSharedParticipants(sessionId));
        Assert.Equal(1, runtime.Demand.GetStreamDemand().SharedParticipantCount);
        Assert.Null(queue.CompletionCause);
        Assert.Equal(0, repository.DecrementCalls);
    }

    [Fact]
    public async Task SweepStaleParticipant_Does_Not_Touch_Replacement_Session()
    {
        var repository = new CountingSessionRepository();
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimes = new LiveSessionStateDirectory(clock);
        var connections = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        using var services = CreateScopedServices(repository, metrics);
        var lifecycleGate = new SessionLifecycleGate();
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimes,
            connections,
            broadcaster,
            metrics,
            clock,
            lifecycleGate: lifecycleGate);
        var sessionId = SessionId.New();
        var staleRuntime = runtimes.CreateRuntime(sessionId);
        staleRuntime.Host.SetSessionStartedAt(clock.GetUtcNow());
        staleRuntime.Participants.TryAddSharedParticipant("viewer", 50, out _);
        var staleQueues = broadcaster.GetOrCreateSession(sessionId);
        staleQueues.AddParticipantQueue("viewer");
        clock.Advance(TimeSpan.FromMinutes(2));

        var gateLease = await lifecycleGate.AcquireAsync(
            sessionId,
            TestContext.Current.CancellationToken);
        var sweeping = service.SweepStaleSharedParticipants(
            sessionId,
            staleRuntime.Participants,
            staleRuntime.Demand,
            TimeSpan.FromMinutes(1),
            TestContext.Current.CancellationToken);
        await WaitForReferenceCountAsync(lifecycleGate, sessionId, 2);
        Assert.True(runtimes.RemoveIfSame(sessionId, staleRuntime));
        Assert.True(broadcaster.RemoveSessionIfSame(sessionId, staleQueues));
        var replacementRuntime = runtimes.CreateRuntime(sessionId);
        replacementRuntime.Host.SetSessionStartedAt(clock.GetUtcNow());
        replacementRuntime.Participants.TryAddSharedParticipant(
            "replacement-viewer",
            50,
            out _);
        var replacementQueues = broadcaster.GetOrCreateSession(sessionId);
        var replacementQueue =
            replacementQueues.AddParticipantQueue("replacement-viewer");
        await gateLease.DisposeAsync();
        await sweeping;

        Assert.Same(replacementRuntime, runtimes.TryGet(sessionId));
        Assert.Same(replacementQueues, broadcaster.TryGetSession(sessionId));
        Assert.Equal(
            1,
            replacementRuntime.Demand.GetStreamDemand().SharedParticipantCount);
        Assert.Null(replacementQueue.CompletionCause);
        Assert.Equal(0, repository.DecrementCalls);
    }

    [Fact]
    public async Task SweepStaleOwnerParticipants_Removes_Stale_OwnerParticipants_And_Updates_Demand()
    {
        var services = new ServiceCollection().BuildServiceProvider();
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connectionRegistry,
            broadcaster,
            metrics,
            clock);
        var sessionId = SessionId.New();
        var userId = UserId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        runtime.Participants.AddOwnerParticipant("owner-console-1");
        connectionRegistry.RegisterOwnerParticipant("owner-console-1", userId, "owner-device", sessionId);
        var sessionQueues = broadcaster.GetOrCreateSession(sessionId);
        sessionQueues.AddParticipantQueue("owner-console-1");
        var hostQueue = sessionQueues.SetHostQueue();
        clock.Advance(TimeSpan.FromMinutes(2));

        await service.SweepStaleOwnerParticipants(
            sessionId,
            runtime.Participants,
            runtime.Demand,
            TimeSpan.FromMinutes(1),
            TestContext.Current.CancellationToken);

        var demand = runtime.Demand.GetStreamDemand();
        Assert.Equal(0, demand.OwnerParticipantCount);
        Assert.False(demand.Required);

        var firstHostMessage = await hostQueue.ReadAsync(CancellationToken.None);
        var secondHostMessage = await hostQueue.ReadAsync(CancellationToken.None);
        Assert.NotNull(firstHostMessage);
        Assert.NotNull(secondHostMessage);
        Assert.Contains("\"reason\":\"owner_participant_timeout\"", System.Text.Encoding.UTF8.GetString(firstHostMessage!));
        Assert.Contains("\"action\":\"left\"", System.Text.Encoding.UTF8.GetString(secondHostMessage!));
    }

    [Fact]
    public async Task SweepAsync_Does_Not_Remove_Runtime_When_End_Persistence_Does_Not_Reach_Target_State()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        using var services = CreateScopedServices(new MissingSessionRepository(), metrics);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connectionRegistry,
            broadcaster,
            metrics,
            clock);
        var sessionId = SessionId.New();
        var runtime = runtimeDirectory.CreateRuntime(sessionId);
        using var missingHostLifetime = new CancellationTokenSource();
        runtime.Host.TryClaimHost("missing-host", missingHostLifetime);
        runtime.Host.ReleaseHost("missing-host");
        runtime.Host.SetStatus(SessionStatus.Reconnecting);
        _ = broadcaster.GetOrCreateSession(sessionId);
        clock.Advance(TimeSpan.FromMinutes(2));

        await service.SweepAsync(CancellationToken.None);

        Assert.Same(runtime, runtimeDirectory.TryGet(sessionId));
        Assert.NotNull(broadcaster.TryGetSession(sessionId));
        Assert.Equal(SessionStatus.Reconnecting, runtime.Host.Status);
    }

    [Fact]
    public async Task SweepAsync_AlreadyEnded_Retires_Matching_Runtime_Without_Duplicate_Projection()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var persistedSession = CreateLiveSession();
        persistedSession.End();
        var userEvents = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var ownerEvents = userEvents.Register(
            "owner-events",
            persistedSession.OwnerUserId,
            "owner-device");
        using var services = CreateScopedServices(
            new StaticSessionRepository(persistedSession),
            metrics,
            userEvents);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connectionRegistry,
            broadcaster,
            metrics,
            clock);
        var runtime = runtimeDirectory.CreateRuntime(persistedSession.Id);
        using var disconnectedHostLifetime = new CancellationTokenSource();
        runtime.Host.TryClaimHost("disconnected-host", disconnectedHostLifetime);
        runtime.Host.ReleaseHost("disconnected-host");
        runtime.Host.SetStatus(SessionStatus.Reconnecting);
        runtime.Host.SetSessionStartedAt(persistedSession.StartedAt);
        var queues = broadcaster.GetOrCreateSession(persistedSession.Id);
        var participant = queues.AddParticipantQueue("participant");
        clock.Advance(TimeSpan.FromMinutes(2));

        await service.SweepAsync(CancellationToken.None);

        Assert.Null(runtimeDirectory.TryGet(persistedSession.Id));
        Assert.Null(broadcaster.TryGetSession(persistedSession.Id));
        Assert.Equal(QueueCompletionCause.SessionEnd, participant.CompletionCause);
        var endedPayload = await participant.ReadAsync(CancellationToken.None);
        var ended = JsonSerializer.Deserialize(
            endedPayload!.Value.Payload,
            WsJsonContext.Default.SessionEndedMessage);
        Assert.Equal(CloseReason.SessionTimeout.ToWire(), ended?.Reason);
        Assert.Null(await participant.ReadAsync(CancellationToken.None));
        using var eventTimeout =
            new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(
            () => ownerEvents.ReadAsync(eventTimeout.Token));
    }

    [Fact]
    public async Task SweepAsync_Ends_Disconnected_Session_When_Persisted_State_Is_Still_Live()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var persistedSession = CreateLiveSession();
        using var services = CreateScopedServices(new StaticSessionRepository(persistedSession), metrics);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connectionRegistry,
            broadcaster,
            metrics,
            clock);
        var runtime = runtimeDirectory.CreateRuntime(persistedSession.Id);
        using var disconnectedHostLifetime = new CancellationTokenSource();
        runtime.Host.TryClaimHost("disconnected-host", disconnectedHostLifetime);
        runtime.Host.ReleaseHost("disconnected-host");
        runtime.Host.SetStatus(SessionStatus.Live);
        runtime.Host.SetSessionStartedAt(persistedSession.StartedAt);
        _ = broadcaster.GetOrCreateSession(persistedSession.Id);
        clock.Advance(TimeSpan.FromMinutes(2));

        await service.SweepAsync(CancellationToken.None);

        Assert.Equal(SessionStatus.Ended, persistedSession.Status);
        Assert.Equal(SessionStatus.Ended, runtime.Host.Status);
        Assert.Null(runtimeDirectory.TryGet(persistedSession.Id));
        Assert.Null(broadcaster.TryGetSession(persistedSession.Id));
    }

    [Fact]
    public async Task SweepAsync_Clears_PreHost_Fences_When_Orphaned_Session_Ends()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var persistedSession = CreateLiveSession();
        var repository = new StaticSessionRepository(persistedSession, includeAsOrphan: true);
        using var services = CreateScopedServices(repository, metrics);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connectionRegistry,
            broadcaster,
            metrics,
            clock);

        broadcaster.GetOrCreateSession(persistedSession.Id);
        broadcaster.NotifyHostKeyDistributionRequested(persistedSession.Id);
        _ = runtimeDirectory.CreateRuntime(persistedSession.Id);

        Assert.Equal(1, PreHostFenceQueueCount(broadcaster));
        Assert.NotNull(runtimeDirectory.TryGet(persistedSession.Id));

        await service.SweepAsync(CancellationToken.None);

        Assert.Equal(SessionStatus.Ended, persistedSession.Status);
        Assert.Null(runtimeDirectory.TryGet(persistedSession.Id));
        Assert.Equal(0, PreHostFenceQueueCount(broadcaster));
        Assert.Equal(1, repository.ClearParticipantCountCalls);
    }

    [Fact]
    public async Task OrphanSweep_Does_Not_End_Republished_Incarnation()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimes = new LiveSessionStateDirectory(clock);
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var session = CreateLiveSession();
        var repository = new StaticSessionRepository(
            session,
            includeAsOrphan: true);
        using var services = CreateScopedServices(repository, metrics);
        var lifecycleGate = new SessionLifecycleGate();
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimes,
            new ConnectionRegistry(),
            broadcaster,
            metrics,
            clock,
            lifecycleGate: lifecycleGate);

        var gateLease = await lifecycleGate.AcquireAsync(
            session.Id,
            TestContext.Current.CancellationToken);
        var sweeping = service.SweepAsync(TestContext.Current.CancellationToken);
        await WaitForReferenceCountAsync(lifecycleGate, session.Id, 2);
        session.End();
        await Task.Delay(1, TestContext.Current.CancellationToken);
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            session.OwnerUserId,
            "republished-orphan",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            "new-secret",
            roomId: null);
        await gateLease.DisposeAsync();
        await sweeping;

        Assert.Equal(SessionStatus.Pending, session.Status);
        Assert.Equal(0, repository.ClearParticipantCountCalls);
    }

    [Fact]
    public async Task SweepAsync_Clears_HostReady_When_Heartbeat_Timeout_Transitions_To_Reconnecting()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var connectionRegistry = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var persistedSession = CreateLiveSession();
        using var services = CreateScopedServices(new StaticSessionRepository(persistedSession), metrics);
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connectionRegistry,
            broadcaster,
            metrics,
            clock);
        var runtime = runtimeDirectory.CreateRuntime(persistedSession.Id);
        const string connectionId = "test-host";
        SetHostSlot(persistedSession, connectionId);
        runtime.Host.TryClaimHost(connectionId, new CancellationTokenSource());
        runtime.Host.SetSessionStartedAt(persistedSession.StartedAt);
        runtime.Host.SetHostReady(true);
        runtime.Host.SetStatus(SessionStatus.Live);
        clock.Advance(TimeSpan.FromMinutes(1));

        await service.SweepAsync(CancellationToken.None);

        Assert.False(runtime.Host.HostConnected);
        Assert.False(runtime.Host.HostReady);
        Assert.Equal(SessionStatus.Reconnecting, runtime.Host.Status);
        Assert.Null(persistedSession.HostConnectionSlot);
    }

    [Fact]
    public async Task ReconnectTimeout_Does_Not_Release_Replacement_Host()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var connections = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var session = CreateLiveSession();
        const string oldConnectionId = "old-host";
        const string replacementConnectionId = "replacement-host";
        SetHostSlot(session, oldConnectionId);
        var repository = new StaticSessionRepository(session);
        using var services = CreateScopedServices(repository, metrics);
        var lifecycleGate = new SessionLifecycleGate();
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connections,
            broadcaster,
            metrics,
            clock,
            lifecycleGate: lifecycleGate);
        var runtime = runtimeDirectory.CreateRuntime(session.Id);
        runtime.Host.TryClaimHost(oldConnectionId, new CancellationTokenSource());
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        runtime.Host.SetStatus(SessionStatus.Live);
        clock.Advance(TimeSpan.FromMinutes(1));

        var gateLease = await lifecycleGate.AcquireAsync(
            session.Id,
            TestContext.Current.CancellationToken);
        var sweeping = service.SweepAsync(TestContext.Current.CancellationToken);
        await WaitForReferenceCountAsync(lifecycleGate, session.Id, 2);
        runtime.Host.ReleaseHost(oldConnectionId);
        SetHostSlot(session, replacementConnectionId);
        Assert.True(runtime.Host.TryClaimHost(
            replacementConnectionId,
            new CancellationTokenSource()));
        await gateLease.DisposeAsync();
        await sweeping;

        Assert.Equal(SessionStatus.Live, session.Status);
        Assert.Equal(SessionStatus.Live, runtime.Host.Status);
        Assert.True(runtime.Host.HostConnected);
        Assert.Equal(replacementConnectionId, runtime.Host.HostConnectionId);
        Assert.Equal(replacementConnectionId, session.HostConnectionSlot);
    }

    [Fact]
    public async Task DisconnectedEndTimeout_Does_Not_End_Republished_Incarnation()
    {
        var clock = new TestTimeProvider(DateTimeOffset.UtcNow);
        var runtimeDirectory = new LiveSessionStateDirectory(clock);
        var connections = new ConnectionRegistry();
        var metrics = new OperationalMetrics(runtimeDirectory);
        var broadcaster = new SessionBroadcaster(
            runtimeDirectory,
            metrics,
            NullLoggerFactory.Instance);
        var session = CreateLiveSession();
        var repository = new StaticSessionRepository(session);
        using var services = CreateScopedServices(repository, metrics);
        var lifecycleGate = new SessionLifecycleGate();
        var service = CreateService(
            services.GetRequiredService<IServiceScopeFactory>(),
            runtimeDirectory,
            connections,
            broadcaster,
            metrics,
            clock,
            lifecycleGate: lifecycleGate);
        var staleRuntime = runtimeDirectory.CreateRuntime(session.Id);
        staleRuntime.Host.TryClaimHost("old-host", new CancellationTokenSource());
        staleRuntime.Host.ReleaseHost("old-host");
        staleRuntime.Host.SetSessionStartedAt(session.StartedAt);
        staleRuntime.Host.SetStatus(SessionStatus.Reconnecting);
        var staleQueues = broadcaster.GetOrCreateSession(session.Id);
        clock.Advance(TimeSpan.FromMinutes(2));

        var gateLease = await lifecycleGate.AcquireAsync(
            session.Id,
            TestContext.Current.CancellationToken);
        var sweeping = service.SweepAsync(TestContext.Current.CancellationToken);
        await WaitForReferenceCountAsync(lifecycleGate, session.Id, 2);
        session.End();
        await Task.Delay(1, TestContext.Current.CancellationToken);
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            session.OwnerUserId,
            "republished-timeout",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            "new-secret",
            roomId: null);
        Assert.True(runtimeDirectory.RemoveIfSame(session.Id, staleRuntime));
        Assert.True(broadcaster.RemoveSessionIfSame(session.Id, staleQueues));
        var replacementRuntime = runtimeDirectory.CreateRuntime(session.Id);
        replacementRuntime.Host.SetSessionStartedAt(session.StartedAt);
        var replacementQueues = broadcaster.GetOrCreateSession(session.Id);
        await gateLease.DisposeAsync();
        await sweeping;

        Assert.Equal(SessionStatus.Pending, session.Status);
        Assert.Same(replacementRuntime, runtimeDirectory.TryGet(session.Id));
        Assert.Same(replacementQueues, broadcaster.TryGetSession(session.Id));
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

    private static HeartbeatTimeoutHostedService CreateService(
        IServiceScopeFactory scopeFactory,
        ILiveSessionStateDirectory runtimes,
        IConnectionRegistry connections,
        SessionBroadcaster broadcaster,
        OperationalMetrics metrics,
        TimeProvider clock,
        RealtimePersistenceRepairTracker? repairTracker = null,
        SessionLifecycleGate? lifecycleGate = null)
    {
        lifecycleGate ??= new SessionLifecycleGate();
        repairTracker ??= new RealtimePersistenceRepairTracker(TimeProvider.System);
        var teardown = new HostTeardown(
            connections,
            runtimes,
            broadcaster,
            scopeFactory,
            metrics,
            lifecycleGate,
            NullLogger<HostTeardown>.Instance,
            repairTracker);
        var coordinator = new SessionEndCoordinator(
            scopeFactory,
            runtimes,
            broadcaster,
            teardown,
            lifecycleGate,
            NullLogger<SessionEndCoordinator>.Instance);
        return new HeartbeatTimeoutHostedService(
            scopeFactory,
            runtimes,
            connections,
            broadcaster,
            coordinator,
            lifecycleGate,
            metrics,
            clock,
            NullLogger<HeartbeatTimeoutHostedService>.Instance,
            repairTracker);
    }

    private static ServiceProvider CreateScopedServices(
        ISessionRepository sessions,
        OperationalMetrics metrics,
        UserEventBroadcaster? userEvents = null)
    {
        return new ServiceCollection()
            .AddScoped<ISessionRepository>(_ => sessions)
            .AddScoped<ISessionKeyBlobRepository>(_ => new FakeSessionKeyBlobRepository())
            .AddScoped<ISessionEndMutationRepository>(
                _ => new FakeSessionEndMutationRepository())
            .AddScoped<IOwnerSessionSecretHasher, OwnerSessionSecretHasher>()
            .AddScoped<IUnitOfWork, NoOpUnitOfWork>()
            .AddScoped<IFriendshipRepository, EmptyFriendshipRepository>()
            .AddScoped<IRoomMemberRepository, EmptyRoomMemberRepository>()
            .AddScoped<ISessionViewerDismissalRepository>(
                _ => new FakeSessionViewerDismissalRepository())
            .AddScoped<UserEventBroadcaster>(
                _ => userEvents
                    ?? new UserEventBroadcaster(
                        metrics,
                        NullLogger<UserEventBroadcaster>.Instance))
            .AddScoped<DiscoveryAudienceResolver>()
            .AddScoped<SharedSurfaceEventPublisher>()
            .AddScoped<LiveSessionTransitionOrchestrator>()
            .AddScoped<LiveSessionStatusReader>()
            .AddScoped<LiveSessionTerminator>()
            .BuildServiceProvider();
    }

    private static Session CreateLiveSession()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Timed Out Session",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            new OwnerSessionSecretHasher().Hash("owner-secret"));
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        return session;
    }

    private static void SetHostSlot(Session session, string connectionId)
    {
        typeof(Session)
            .GetProperty(nameof(Session.HostConnectionSlot))!
            .SetValue(session, connectionId);
    }

    private sealed class MissingSessionRepository : SessionRepositoryStub
    {
        public override Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)
            => Task.FromResult<Session?>(null);

        public override Task<Session?> GetByIdForUpdateAsync(
            SessionId id,
            CancellationToken ct = default)
            => Task.FromResult<Session?>(null);

        public override Task AddAsync(Session session, CancellationToken ct = default) => Task.CompletedTask;
        public override Task UpdateAsync(Session session, CancellationToken ct = default) => Task.CompletedTask;

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

    private sealed class CountingSessionRepository(int? throwOnCall = null)
        : SessionRepositoryStub
    {
        private readonly int? _throwOnCall = throwOnCall;

        public int DecrementCalls { get; private set; }
        public List<(SessionId SessionId, int Count)> ParticipantCounts { get; } = [];
        public List<SessionId> ParticipantCountClears { get; } = [];


        public override Task<bool> TryDecrementParticipantCountAsync(
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            CancellationToken ct = default)
        {
            DecrementCalls++;
            if (_throwOnCall is int failAt && DecrementCalls == failAt)
            {
                throw new InvalidOperationException(
                    $"simulated DB failure on decrement {failAt}");
            }

            return Task.FromResult(true);
        }


        public override Task<bool> TrySetParticipantCountAsync(
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            int participantCount,
            CancellationToken ct = default)
        {
            ParticipantCounts.Add((sessionId, participantCount));
            return Task.FromResult(true);
        }

        public override Task<bool> TryClearParticipantCountAsync(
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            CancellationToken ct = default)
        {
            ParticipantCountClears.Add(sessionId);
            return Task.FromResult(true);
        }

        public override Task<IReadOnlyList<OrphanedSessionCandidate>>
            GetOrphanedLiveSessionsAsync(
            DateTimeOffset noHostSince,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<OrphanedSessionCandidate>>([]);

        public override Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)
            => Task.FromResult<Session?>(null);

        public override Task AddAsync(Session session, CancellationToken ct = default) => Task.CompletedTask;
        public override Task UpdateAsync(Session session, CancellationToken ct = default) => Task.CompletedTask;

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

    private sealed class RepairSessionRepository : SessionRepositoryStub
    {
        public List<(SessionId SessionId, string ConnectionId)> HostReleases { get; } = [];
        public List<(SessionId SessionId, int Count)> ParticipantCounts { get; } = [];
        public List<SessionId> ParticipantCountClears { get; } = [];

        public override Task ReleaseHostSlotAsync(
            SessionId sessionId,
            string? connectionId,
            CancellationToken ct = default)
        {
            HostReleases.Add((sessionId, connectionId!));
            return Task.CompletedTask;
        }


        public override Task<bool> TrySetParticipantCountAsync(
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            int participantCount,
            CancellationToken ct = default)
        {
            ParticipantCounts.Add((sessionId, participantCount));
            return Task.FromResult(true);
        }

        public override Task<bool> TryClearParticipantCountAsync(
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            CancellationToken ct = default)
        {
            ParticipantCountClears.Add(sessionId);
            return Task.FromResult(true);
        }

        public override Task<Session?> GetByIdAsync(
            SessionId id,
            CancellationToken ct = default) =>
            Task.FromResult<Session?>(null);

        public override Task AddAsync(Session session, CancellationToken ct = default) =>
            Task.CompletedTask;

        public override Task UpdateAsync(Session session, CancellationToken ct = default) =>
            Task.CompletedTask;

        public override Task<IReadOnlyList<SessionCardProjection>> GetByOwnerProjectedAsync(
            UserId ownerUserId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SessionCardProjection>>([]);

        public override Task<IReadOnlyList<SessionCardProjection>> GetLiveByRoomProjectedAsync(
            RoomId roomId,
            FeedCursor? cursor = null,
            int limit = 20,
            ToolKind? toolKindFilter = null,
            DateTimeOffset? since = null,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SessionCardProjection>>([]);

    }

    private static int PreHostFenceQueueCount(SessionBroadcaster broadcaster)
    {
        var fencesField = typeof(SessionBroadcaster)
            .GetField("_fences", BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.NotNull(fencesField);
        var fences = fencesField.GetValue(broadcaster);
        Assert.NotNull(fences);
        var preHostQueuesField = fences.GetType()
            .GetField("_preHostFenceQueues", BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.NotNull(preHostQueuesField);
        var preHostFenceQueues = Assert.IsType<ConcurrentDictionary<SessionId, PendingHostFenceQueue>>(
            preHostQueuesField.GetValue(fences));
        return preHostFenceQueues.Count;
    }

    private sealed class BlockingRepairSessionRepository
        : SessionRepositoryStub
    {
        private readonly TaskCompletionSource _release =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource WriteStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public List<(SessionId, DateTimeOffset, int)> Writes { get; } = [];

        public async override Task<bool> TrySetParticipantCountAsync(
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            int participantCount,
            CancellationToken ct = default)
        {
            WriteStarted.TrySetResult();
            await _release.Task.WaitAsync(ct);
            Writes.Add((sessionId, expectedStartedAt, participantCount));
            return true;
        }

        public override Task<IReadOnlyList<OrphanedSessionCandidate>>
            GetOrphanedLiveSessionsAsync(
                DateTimeOffset noHostSince,
                CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<OrphanedSessionCandidate>>([]);

        public void Release() => _release.TrySetResult();
    }

    private sealed class StaticSessionRepository(Session session, bool includeAsOrphan = false)
        : SessionRepositoryStub
    {
        private readonly Session _session = session;
        private readonly bool _includeAsOrphan = includeAsOrphan;

        public int ClearParticipantCountCalls { get; private set; }

        public override Task<Session?> GetByIdAsync(SessionId id, CancellationToken ct = default)
            => Task.FromResult(id == _session.Id ? _session : null);

        public override Task<Session?> GetByIdForUpdateAsync(
            SessionId id,
            CancellationToken ct = default)
            => Task.FromResult(id == _session.Id ? _session : null);

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

        public override Task<IReadOnlyList<OrphanedSessionCandidate>>
            GetOrphanedLiveSessionsAsync(
            DateTimeOffset noHostSince,
            CancellationToken ct = default)
            => Task.FromResult<IReadOnlyList<OrphanedSessionCandidate>>(
                _includeAsOrphan
                    ? [new OrphanedSessionCandidate(
                        _session.Id,
                        _session.StartedAt,
                        _session.HostReleasedAt,
                        _session.LastHeartbeatAt)]
                    : []);

        public override Task ClearParticipantCountAsync(
            SessionId sessionId,
            CancellationToken ct = default)
        {
            if (sessionId == _session.Id)
            {
                ClearParticipantCountCalls++;
            }

            return Task.CompletedTask;
        }

        public override async Task<bool> TryClearParticipantCountAsync(
            SessionId sessionId,
            DateTimeOffset expectedStartedAt,
            CancellationToken ct = default)
        {
            if (sessionId != _session.Id
                || expectedStartedAt != _session.StartedAt)
            {
                return false;
            }

            await ClearParticipantCountAsync(sessionId, ct);
            return true;
        }
    }

    private sealed class NoOpUnitOfWork : UnitOfWorkStub
    {
        public override Task SaveChangesAsync(CancellationToken ct = default) => Task.CompletedTask;

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default) =>
            Task.FromResult<ITransactionScope>(new CompletedTransactionScope());
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

    private sealed class ClosedWebSocket : WebSocket
    {
        public override WebSocketCloseStatus? CloseStatus => null;
        public override string? CloseStatusDescription => null;
        public override WebSocketState State => WebSocketState.Closed;
        public override string? SubProtocol => null;

        public override void Abort()
        {
        }

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

        public override void Dispose()
        {
        }

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
}
