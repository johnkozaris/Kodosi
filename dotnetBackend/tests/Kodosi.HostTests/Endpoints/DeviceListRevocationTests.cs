using System.Text;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;
using Microsoft.Extensions.Logging.Abstractions;

namespace Kodosi.HostTests;

public sealed class DeviceListRevocationTests
{
    [Fact]
    public void IdentityResetFence_Covers_Session_And_UserEvent_Registrations()
    {
        var userId = UserId.New();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var registry = new ConnectionRegistry();
        var userEvents = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var enforcer = new IdentityResetRealtimeEnforcer(new RealtimeDeviceAccessEnforcementCore(
            registry,
            new SessionBroadcaster(
                runtimes,
                metrics,
                NullLoggerFactory.Instance),
            userEvents));
        using var hostLifetime = new CancellationTokenSource();

        using (enforcer.FenceNewConnections(userId, ["reset-device"]))
        {
            registry.RegisterHost(
                "late-host",
                userId,
                "reset-device",
                SessionId.New(),
                hostLifetime);
            var queue = userEvents.Register(
                "late-events",
                userId,
                "reset-device");

            Assert.True(hostLifetime.IsCancellationRequested);
            Assert.Equal(CloseReason.AccessRevoked, queue.CompletionReason);
            Assert.Equal(0, userEvents.ActiveConnectionCount);
        }
    }

    [Fact]
    public async Task PersistCommitAndEnforceRevocation_Holds_Sorted_Affected_Gates_Through_Invalidation()
    {
        var userId = UserId.New();
        var firstSessionId = SessionId.From(
            Guid.Parse("10000000-0000-0000-0000-000000000001"));
        var secondSessionId = SessionId.From(
            Guid.Parse("20000000-0000-0000-0000-000000000002"));
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var registry = new ConnectionRegistry();
        var userEvents = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var firstRuntime = runtimes.CreateRuntime(firstSessionId);
        var firstIncarnationId = Guid.NewGuid();
        firstRuntime.Host.SetSessionIncarnationId(firstIncarnationId);
        broadcaster.GetOrCreateSession(firstSessionId).SetHostQueue("first-host");
        var runtime = runtimes.CreateRuntime(secondSessionId);
        runtime.Host.SetStatus(SessionStatus.Live);
        runtime.Host.SetHostReady(true);
        var queues = broadcaster.GetOrCreateSession(secondSessionId);
        queues.SetHostQueue("host");
        var participantQueue = queues.AddParticipantQueue(
            "revoked-participant",
            AccessLevel.Suggest);
        registry.RegisterSharedParticipant(
            "revoked-participant",
            userId,
            "revoked-device",
            secondSessionId);
        var lifecycleGate = new SessionLifecycleGate();
        var authority = new RecordingGateAuthority(lifecycleGate);
        var resolver = new FakeDeviceRevocationSessionResolver(
            new DeviceRevocationSessionTarget(
                firstSessionId,
                firstIncarnationId));
        var persisted = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var allowInvalidation = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        using var lateConnectionLifetime = new CancellationTokenSource();
        ChannelByteSendQueue? lateUserEventQueue = null;

        var effects = DeviceListRealtimeEffectsFactory.Create(
            registry,
            broadcaster,
            userEvents,
            authority,
            resolver,
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);
        var enforcing = effects.PersistAndEnforceAsync(
            userId,
            ["revoked-device"],
            [new DeviceRevocationSessionTarget(firstSessionId, firstIncarnationId)],
            async ct =>
            {
                registry.RegisterSharedParticipant(
                    "late-revoked-participant",
                    userId,
                    "revoked-device",
                    firstSessionId,
                    lateConnectionLifetime);
                lateUserEventQueue = userEvents.Register(
                    "late-user-event",
                    userId,
                    "revoked-device");
                Assert.True(lateConnectionLifetime.IsCancellationRequested);
                Assert.Equal(
                    CloseReason.AccessRevoked,
                    lateUserEventQueue.CompletionReason);
                persisted.TrySetResult();
                await allowInvalidation.Task.WaitAsync(ct);
            },
            TestContext.Current.CancellationToken);
        await persisted.Task.WaitAsync(TestContext.Current.CancellationToken);

        var dispatchCalls = 0;
        var actionAuthority = new ParticipantActionAuthority(
            secondSessionId,
            "revoked-participant",
            userId,
            "revoked-device",
            new ParticipantAccessDecision(
                IsOwnerParticipant: false,
                AccessLevel: AccessLevel.Suggest),
            runtime,
            queues,
            participantQueue,
            runtimes,
            registry,
            broadcaster,
            lifecycleGate);
        var dispatching = actionAuthority.TryDispatchAsync(
                SessionCapability.Suggest,
                () =>
                {
                    Interlocked.Increment(ref dispatchCalls);
                    return broadcaster.SendToHost(secondSessionId, [1]);
                },
                TestContext.Current.CancellationToken)
            .AsTask();
        await WaitForReferenceCountAsync(
            lifecycleGate,
            secondSessionId,
            expected: 2);
        Assert.False(dispatching.IsCompleted);

        allowInvalidation.TrySetResult();
        await enforcing;
        var dispatch = await dispatching;

        Assert.Equal([firstSessionId, secondSessionId], authority.AcquiredSessionIds);
        var resolverCall = Assert.Single(resolver.Calls);
        Assert.Equal(
            [new DeviceRevocationSessionTarget(firstSessionId, firstIncarnationId)],
            resolverCall);
        Assert.False(dispatch.Authorized);
        Assert.Equal(0, dispatchCalls);
        Assert.Equal(
            QueueCompletionCause.AccessRevokedCascade,
            participantQueue.CompletionCause);
        Assert.NotNull(lateUserEventQueue);
        Assert.Null(await lateUserEventQueue.ReadAsync(CancellationToken.None));
    }

    [Fact]
    public async Task Immediate_Revocation_Does_Not_Fence_Republished_Incarnation()
    {
        var userId = UserId.New();
        var sessionId = SessionId.New();
        var recordedIncarnationId = Guid.NewGuid();
        var replacementIncarnationId = Guid.NewGuid();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var runtime = runtimes.CreateRuntime(sessionId);
        runtime.Host.SetSessionIncarnationId(replacementIncarnationId);
        var replacementHostQueue = broadcaster
            .GetOrCreateSession(sessionId)
            .SetHostQueue("replacement-host");
        var resolver = new FakeDeviceRevocationSessionResolver();
        var authority = new RecordingGateAuthority(new SessionLifecycleGate());
        var effects = DeviceListRealtimeEffectsFactory.Create(
            new ConnectionRegistry(),
            broadcaster,
            new UserEventBroadcaster(
                metrics,
                NullLogger<UserEventBroadcaster>.Instance),
            authority,
            resolver,
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);

        await effects.PersistAndEnforceAsync(
            userId,
            ["revoked-device"],
            [new DeviceRevocationSessionTarget(sessionId, recordedIncarnationId)],
            _ => Task.CompletedTask,
            TestContext.Current.CancellationToken);

        Assert.Null(replacementHostQueue.CompletionReason);
        Assert.Empty(authority.AcquiredSessionIds);
        var resolverCall = Assert.Single(resolver.Calls);
        Assert.Equal(
            [new DeviceRevocationSessionTarget(sessionId, recordedIncarnationId)],
            resolverCall);
    }

    [Fact]
    public async Task Immediate_Revocation_Revalidates_Target_After_PreGate_Read()
    {
        var userId = UserId.New();
        var sessionId = SessionId.New();
        var recordedIncarnationId = Guid.NewGuid();
        var replacementIncarnationId = Guid.NewGuid();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var runtime = runtimes.CreateRuntime(sessionId);
        runtime.Host.SetSessionIncarnationId(recordedIncarnationId);
        var hostQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue("host");
        var authority = new RecordingGateAuthority(new SessionLifecycleGate());
        var resolver = new MutatingSessionResolver(
            runtimes,
            sessionId,
            replacementIncarnationId);
        var effects = DeviceListRealtimeEffectsFactory.Create(
            new ConnectionRegistry(),
            broadcaster,
            new UserEventBroadcaster(
                metrics,
                NullLogger<UserEventBroadcaster>.Instance),
            authority,
            resolver,
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);

        await effects.PersistAndEnforceAsync(
            userId,
            ["revoked-device"],
            [new DeviceRevocationSessionTarget(sessionId, recordedIncarnationId)],
            _ => Task.CompletedTask,
            TestContext.Current.CancellationToken);

        Assert.Equal(1, resolver.CallCount);
        Assert.Equal([sessionId], authority.AcquiredSessionIds);
        Assert.Null(hostQueue.CompletionReason);
    }

    [Fact]
    public async Task Revocation_With_Queue_But_No_Runtime_Queues_Pending_Host_Fence()
    {
        var userId = UserId.New();
        var sessionId = SessionId.New();
        var incarnationId = Guid.NewGuid();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        broadcaster.GetOrCreateSession(sessionId);
        var target = new DeviceRevocationSessionTarget(sessionId, incarnationId);
        var effects = DeviceListRealtimeEffectsFactory.Create(
            new ConnectionRegistry(),
            broadcaster,
            new UserEventBroadcaster(
                metrics,
                NullLogger<UserEventBroadcaster>.Instance),
            new RecordingGateAuthority(new SessionLifecycleGate()),
            new FakeDeviceRevocationSessionResolver(target),
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);

        await effects.EnforceCommittedAsync(
            userId,
            ["revoked-device"],
            [target],
            TestContext.Current.CancellationToken);

        var runtime = runtimes.CreateRuntime(sessionId);
        runtime.Host.SetSessionIncarnationId(incarnationId);
        var hostQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue("host");
        broadcaster.FlushPendingHostFences(sessionId);
        var payload = await hostQueue.ReadAsync(TestContext.Current.CancellationToken);
        Assert.Contains(
            "\"type\":\"host.accessRevoked\"",
            Encoding.UTF8.GetString(payload!));
    }

    [Fact]
    public async Task Revocation_With_Runtime_But_No_Queue_Queues_Pending_Host_Fence()
    {
        var userId = UserId.New();
        var sessionId = SessionId.New();
        var incarnationId = Guid.NewGuid();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var runtime = runtimes.CreateRuntime(sessionId);
        runtime.Host.SetSessionIncarnationId(incarnationId);
        var target = new DeviceRevocationSessionTarget(sessionId, incarnationId);
        var effects = DeviceListRealtimeEffectsFactory.Create(
            new ConnectionRegistry(),
            broadcaster,
            new UserEventBroadcaster(
                metrics,
                NullLogger<UserEventBroadcaster>.Instance),
            new RecordingGateAuthority(new SessionLifecycleGate()),
            new FakeDeviceRevocationSessionResolver(target),
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);

        await effects.EnforceCommittedAsync(
            userId,
            ["revoked-device"],
            [target],
            TestContext.Current.CancellationToken);

        var hostQueue = broadcaster.GetOrCreateSession(sessionId).SetHostQueue("host");
        broadcaster.FlushPendingHostFences(sessionId);
        var payload = await hostQueue.ReadAsync(TestContext.Current.CancellationToken);
        Assert.Contains(
            "\"type\":\"host.accessRevoked\"",
            Encoding.UTF8.GetString(payload!));
    }

    [Fact]
    public async Task EnforceCommittedRevocation_DisconnectsDeviceAndRotatesOfflineKeySessions()
    {
        var userId = UserId.New();
        var onlineSessionId = SessionId.New();
        var offlineSessionId = SessionId.New();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var registry = new ConnectionRegistry();

        var onlineQueues = broadcaster.GetOrCreateSession(onlineSessionId);
        var onlineHostQueue = onlineQueues.SetHostQueue();
        var revokedParticipantQueue = onlineQueues.AddParticipantQueue("revoked-viewer");
        registry.RegisterSharedParticipant(
            "revoked-viewer",
            userId,
            "revoked-device",
            onlineSessionId);

        var offlineRuntime = runtimes.CreateRuntime(offlineSessionId);
        var offlineIncarnationId = Guid.NewGuid();
        offlineRuntime.Host.SetSessionIncarnationId(offlineIncarnationId);
        var offlineHostQueue = broadcaster
            .GetOrCreateSession(offlineSessionId)
            .SetHostQueue();

        var effects = DeviceListRealtimeEffectsFactory.Create(
            registry,
            broadcaster,
            new UserEventBroadcaster(metrics, NullLogger<UserEventBroadcaster>.Instance),
            new RecordingGateAuthority(new SessionLifecycleGate()),
            new FakeDeviceRevocationSessionResolver(
                new DeviceRevocationSessionTarget(
                    offlineSessionId,
                    offlineIncarnationId)),
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);
        await effects.EnforceCommittedAsync(
            userId,
            ["revoked-device"],
            [new DeviceRevocationSessionTarget(offlineSessionId, offlineIncarnationId)],
            TestContext.Current.CancellationToken);

        Assert.Equal(
            QueueCompletionCause.AccessRevokedCascade,
            revokedParticipantQueue.CompletionCause);
        Assert.Contains(
            "\"type\":\"host.accessRevoked\"",
            Encoding.UTF8.GetString((await onlineHostQueue.ReadAsync(CancellationToken.None))!));
        Assert.Contains(
            "\"type\":\"host.accessRevoked\"",
            Encoding.UTF8.GetString((await offlineHostQueue.ReadAsync(CancellationToken.None))!));
    }

    [Fact]
    public async Task EnforceCommittedRevocation_ClosesForeignSessionHostWithTheGivenReason()
    {
        var userId = UserId.New();
        var foreignSessionId = SessionId.New();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var registry = new ConnectionRegistry();

        var hostQueue = broadcaster.GetOrCreateSession(foreignSessionId).SetHostQueue();
        registry.RegisterHost("host-conn", userId, "reset-device", foreignSessionId);

        var core = new RealtimeDeviceAccessEnforcementCore(
            registry,
            broadcaster,
            new UserEventBroadcaster(metrics, NullLogger<UserEventBroadcaster>.Instance));
        core.EnforceCommitted(
            userId,
            ["reset-device"],
            [],
            CloseReason.OwnerIdentityReset);




        Assert.Equal(CloseReason.OwnerIdentityReset, hostQueue.CompletionReason);
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

    private sealed class MutatingSessionResolver(
        LiveSessionStateDirectory runtimes,
        SessionId sessionId,
        Guid replacementIncarnationId)
        : IDeviceRevocationSessionResolver
    {
        public int CallCount { get; private set; }

        public Task<IReadOnlyList<DeviceRevocationSessionTarget>> GetCurrentTargetsAsync(
            IReadOnlyCollection<DeviceRevocationSessionTarget> targets,
            CancellationToken ct = default)
        {
            ct.ThrowIfCancellationRequested();
            CallCount++;
            if (CallCount == 1)
            {
                runtimes.TryGet(sessionId)!.Host.SetSessionIncarnationId(
                    replacementIncarnationId);
            }
            return Task.FromResult<IReadOnlyList<DeviceRevocationSessionTarget>>(
                targets.ToList());
        }
    }

    private sealed class RecordingGateAuthority(
        SessionLifecycleGate lifecycleGate) : ISessionEndAuthority
    {
        public IReadOnlyList<SessionId> AcquiredSessionIds { get; private set; } = [];

        public ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default)
        {
            AcquiredSessionIds = [.. sessionIds];
            return lifecycleGate.AcquireAsync(sessionIds, ct);
        }

        public Task ProjectCommittedAsync(
            IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task RetireEndedIncarnationAsync(
            SessionDiscoveryTarget endedSession,
            DateTimeOffset startedAt,
            CancellationToken ct = default) =>
            Task.CompletedTask;
    }
}
