using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;
using Microsoft.Extensions.Logging.Abstractions;

namespace Kodosi.HostTests;

public sealed class DeviceRevocationEnforcementHostedServiceTests
{
    [Fact]
    public async Task Pending_Revocation_Disconnects_Device_Rotates_Sessions_And_Completes()
    {
        var userId = UserId.New();
        var sessionId = SessionId.New();
        var revocationId = Guid.NewGuid();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var sessions = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var connections = new ConnectionRegistry();
        var userEvents = new UserEventBroadcaster(
            metrics,
            NullLogger<UserEventBroadcaster>.Instance);
        var queues = sessions.GetOrCreateSession(sessionId);
        var hostQueue = queues.SetHostQueue("host");
        var runtime = runtimes.CreateRuntime(sessionId);
        var sessionIncarnationId = Guid.NewGuid();
        runtime.Host.SetSessionIncarnationId(sessionIncarnationId);
        queues.AddParticipantQueue("revoked", AccessLevel.Approve);
        using var revokedCancellation = new CancellationTokenSource();
        connections.RegisterSharedParticipant(
            "revoked",
            userId,
            "revoked-device",
            sessionId,
            revokedCancellation);
        var durability = new FakeDurability(
            new DeviceRevocationEnforcementWork(
                revocationId,
                userId,
                1,
                2,
                ["revoked-device"],
                [new DeviceRevocationSessionTarget(sessionId, sessionIncarnationId)]));
        var authority = new LeaseAuthority();
        var effects = DeviceListRealtimeEffectsFactory.Create(
            connections,
            sessions,
            userEvents,
            authority,
            new FakeDeviceRevocationSessionResolver(
                new DeviceRevocationSessionTarget(
                    sessionId,
                    sessionIncarnationId)),
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);
        var worker = new DeviceRevocationEnforcementHostedService(
            durability,
            effects,
            NullLogger<DeviceRevocationEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.True(authority.Acquired);
        Assert.True(authority.LeaseReleased);
        Assert.Equal([revocationId], durability.Completed);
        Assert.True(revokedCancellation.IsCancellationRequested);
        var hostMessage = await hostQueue.ReadAsync(TestContext.Current.CancellationToken);
        Assert.NotNull(hostMessage);
    }

    [Fact]
    public async Task Pending_Revocation_For_Previous_Incarnation_Completes_Without_Fencing_Replacement()
    {
        var userId = UserId.New();
        var sessionId = SessionId.New();
        var revocationId = Guid.NewGuid();
        var recordedIncarnationId = Guid.NewGuid();
        var replacementIncarnationId = Guid.NewGuid();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var sessions = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var runtime = runtimes.CreateRuntime(sessionId);
        runtime.Host.SetSessionIncarnationId(replacementIncarnationId);
        var replacementHostQueue = sessions
            .GetOrCreateSession(sessionId)
            .SetHostQueue("replacement-host");
        var durability = new FakeDurability(
            new DeviceRevocationEnforcementWork(
                revocationId,
                userId,
                1,
                2,
                ["revoked-device"],
                [new DeviceRevocationSessionTarget(
                    sessionId,
                    recordedIncarnationId)]));
        var effects = DeviceListRealtimeEffectsFactory.Create(
            new ConnectionRegistry(),
            sessions,
            new UserEventBroadcaster(
                metrics,
                NullLogger<UserEventBroadcaster>.Instance),
            new LeaseAuthority(),
            new FakeDeviceRevocationSessionResolver(
                new DeviceRevocationSessionTarget(
                    sessionId,
                    replacementIncarnationId)),
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);
        var worker = new DeviceRevocationEnforcementHostedService(
            durability,
            effects,
            NullLogger<DeviceRevocationEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.Equal([revocationId], durability.Completed);
        Assert.Null(replacementHostQueue.CompletionReason);
    }

    [Fact]
    public async Task Pending_Revocation_With_Empty_Targets_Completes_Without_Current_Session_Fanout()
    {
        var userId = UserId.New();
        var sessionId = SessionId.New();
        var revocationId = Guid.NewGuid();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var sessions = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var runtime = runtimes.CreateRuntime(sessionId);
        runtime.Host.SetSessionIncarnationId(Guid.NewGuid());
        var currentHostQueue = sessions
            .GetOrCreateSession(sessionId)
            .SetHostQueue("current-host");
        var durability = new FakeDurability(
            new DeviceRevocationEnforcementWork(
                revocationId,
                userId,
                1,
                2,
                ["revoked-device"],
                []));
        var effects = DeviceListRealtimeEffectsFactory.Create(
            new ConnectionRegistry(),
            sessions,
            new UserEventBroadcaster(
                metrics,
                NullLogger<UserEventBroadcaster>.Instance),
            new LeaseAuthority(),
            new FakeDeviceRevocationSessionResolver(),
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);
        var worker = new DeviceRevocationEnforcementHostedService(
            durability,
            effects,
            NullLogger<DeviceRevocationEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.Equal([revocationId], durability.Completed);
        Assert.Null(currentHostQueue.CompletionReason);
    }

    [Fact]
    public async Task Timed_Out_Revocation_Enforcement_Remains_Pending_For_Retry()
    {
        var work = new DeviceRevocationEnforcementWork(
            Guid.NewGuid(),
            UserId.New(),
            1,
            2,
            ["revoked-device"],
            []);
        var durability = new FakeDurability(work)
        {
            BlockUntilCancelled = true,
        };
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var sessions = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var effects = DeviceListRealtimeEffectsFactory.Create(
            new ConnectionRegistry(),
            sessions,
            new UserEventBroadcaster(metrics, NullLogger<UserEventBroadcaster>.Instance),
            new LeaseAuthority(),
            new FakeDeviceRevocationSessionResolver(),
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);
        var worker = new DeviceRevocationEnforcementHostedService(
            durability,
            effects,
            NullLogger<DeviceRevocationEnforcementHostedService>.Instance,
            TimeProvider.System)
        {
            EnforcementAttemptTimeout = TimeSpan.FromMilliseconds(20),
        };

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.True(durability.AttemptWasCancelled);
        Assert.Empty(durability.Completed);
    }

    [Fact]
    public async Task Obsolete_Revocation_Completes_Without_Disconnecting_Current_Device()
    {
        var userId = UserId.New();
        var sessionId = SessionId.New();
        var revocationId = Guid.NewGuid();
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var sessions = new SessionBroadcaster(runtimes, metrics, NullLoggerFactory.Instance);
        var connections = new ConnectionRegistry();
        using var currentCancellation = new CancellationTokenSource();
        connections.RegisterSharedParticipant(
            "current",
            userId,
            "reused-device",
            sessionId,
            currentCancellation);
        var durability = new FakeDurability(
            new DeviceRevocationEnforcementWork(
                revocationId,
                userId,
                1,
                2,
                ["reused-device"],
                [new DeviceRevocationSessionTarget(sessionId, Guid.NewGuid())]))
        {
            IsCurrent = false,
        };
        var authority = new LeaseAuthority();
        var effects = DeviceListRealtimeEffectsFactory.Create(
            connections,
            sessions,
            new UserEventBroadcaster(metrics, NullLogger<UserEventBroadcaster>.Instance),
            authority,
            new FakeDeviceRevocationSessionResolver(),
            runtimes,
            NullLogger<DeviceListRealtimeEffects>.Instance);
        var worker = new DeviceRevocationEnforcementHostedService(
            durability,
            effects,
            NullLogger<DeviceRevocationEnforcementHostedService>.Instance,
            TimeProvider.System);

        await worker.EnforcePendingAsync(TestContext.Current.CancellationToken);

        Assert.Equal([revocationId], durability.Completed);
        Assert.False(currentCancellation.IsCancellationRequested);
        Assert.False(authority.Acquired);
    }

    private sealed class FakeDurability(params DeviceRevocationEnforcementWork[] work)
        : IDeviceRevocationDurabilityCoordinator
    {
        public List<Guid> Completed { get; } = [];
        public bool IsCurrent { get; init; } = true;
        public bool BlockUntilCancelled { get; init; }
        public bool AttemptWasCancelled { get; private set; }

        public Task<IReadOnlyList<DeviceRevocationEnforcementWork>> GetPendingEnforcementAsync(
            int limit,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<DeviceRevocationEnforcementWork>>(work.Take(limit).ToList());

        public async Task<bool> ExecuteIfCurrentAsync(
            DeviceRevocationEnforcementWork work,
            Func<CancellationToken, Task> enforce,
            CancellationToken ct = default)
        {
            if (!IsCurrent)
            {
                return false;
            }
            if (BlockUntilCancelled)
            {
                try
                {
                    await Task.Delay(Timeout.InfiniteTimeSpan, ct);
                }
                catch (OperationCanceledException) when (ct.IsCancellationRequested)
                {
                    AttemptWasCancelled = true;
                    throw;
                }
            }
            await enforce(ct);
            return true;
        }

        public Task CompleteEnforcementAsync(Guid revocationId, CancellationToken ct = default)
        {
            Completed.Add(revocationId);
            return Task.CompletedTask;
        }
    }

    private sealed class LeaseAuthority : ISessionEndAuthority
    {
        public bool Acquired { get; private set; }
        public bool LeaseReleased { get; private set; }

        public ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default)
        {
            Acquired = true;
            return ValueTask.FromResult<IAsyncDisposable>(new Lease(this));
        }

        public Task ProjectCommittedAsync(
            IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
            CancellationToken ct = default) => Task.CompletedTask;

        public Task RetireEndedIncarnationAsync(
            SessionDiscoveryTarget endedSession,
            DateTimeOffset startedAt,
            CancellationToken ct = default) => Task.CompletedTask;

        private sealed class Lease(LeaseAuthority owner) : IAsyncDisposable
        {
            public ValueTask DisposeAsync()
            {
                owner.LeaseReleased = true;
                return ValueTask.CompletedTask;
            }
        }
    }
}
