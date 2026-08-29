using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;

namespace Kodosi.HostTests;

public sealed class CleanupHostedServiceTests
{
    [Fact]
    public async Task Semantic_Receipt_Sweep_Uses_The_24_Hour_Cutoff_And_Bounded_Batch()
    {
        var now = new DateTimeOffset(2026, 8, 16, 12, 0, 0, TimeSpan.Zero);
        var semantic = new RecordingSemanticRelayRepository();
        var services = new ServiceCollection();
        services.AddSingleton<ISemanticRelayRepository>(semantic);
        await using var provider = services.BuildServiceProvider();
        var service = new CleanupHostedService(
            new NoopActionDedupeCache(),
            new NoopFriendRequestThrottle(),
            new NoopDeviceLinkPollThrottle(),
            provider.GetRequiredService<IServiceScopeFactory>(),
            new GateOnlySessionEndAuthority(new SessionLifecycleGate()),
            NullLogger<CleanupHostedService>.Instance,
            new TestTimeProvider(now));

        await service.SweepAcknowledgedSemanticReceiptsAsync(
            now,
            TestContext.Current.CancellationToken);

        Assert.Equal(now.AddHours(-24), semantic.Cutoff);
        Assert.Equal(1_000, semantic.Limit);
    }

    [Fact]
    public async Task ExpiredAccess_Holds_Lifecycle_Gate_From_Commit_Through_Queue_Completion()
    {
        var now = DateTimeOffset.UtcNow;
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "expired-access",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var accessOverride = SessionAccessOverride.Create(
            session.Id,
            viewerId,
            AccessLevel.Suggest,
            ownerId,
            expiresAt: now.AddMinutes(-1),
            createdAt: now.AddMinutes(-2));
        var overrides = new FakeAccessOverrideRepository(accessOverride);
        var audit = new FakeAccessOverrideAuditRepository();
        var durability = new RecordingAccessOverrideExpiryDurabilityCoordinator();
        var sessions = new BlockingFreshScopeSessionRepository(session);
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var registry = new ConnectionRegistry();
        var gate = new SessionLifecycleGate();

        var runtime = runtimes.CreateRuntime(session.Id);
        runtime.Host.SetSessionIncarnationId(session.IncarnationId);
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        runtime.Host.SetStatus(SessionStatus.Live);
        runtime.Host.SetHostReady(true);
        var queues = broadcaster.GetOrCreateSession(session.Id);
        queues.SetHostQueue("host");
        var participantQueue = queues.AddParticipantQueue(
            "expired-viewer",
            AccessLevel.Suggest);
        registry.RegisterSharedParticipant(
            "expired-viewer",
            viewerId,
            "viewer-device",
            session.Id);

        var services = new ServiceCollection();
        services.AddLogging();
        services.AddSingleton<IAccessOverrideRepository>(overrides);
        services.AddSingleton<IAccessOverrideAuditRepository>(audit);
        services.AddSingleton<ISessionRepository>(sessions);
        services.AddSingleton<IUnitOfWork>(new FakeUnitOfWork());
        services.AddSingleton<IAccessOverrideExpiryDurabilityCoordinator>(durability);
        services.AddSingleton<IFriendshipRepository>(
            new FakeFriendshipRepository());
        services.AddSingleton<IRoomMemberRepository>(
            new FakeRoomMemberRepository());
        services.AddSingleton<ISessionViewerDismissalRepository>(
            new FakeSessionViewerDismissalRepository());
        services.AddSingleton<ILiveSessionStateDirectory>(runtimes);
        services.AddSingleton<IConnectionRegistry>(registry);
        services.AddSingleton(broadcaster);
        services.AddSingleton(gate);
        services.AddSingleton(metrics);
        services.AddScoped<SessionAccessService>();
        services.AddScoped<SessionReader>();
        services.AddScoped<SessionAccessDisconnector>();
        await using var provider = services.BuildServiceProvider();

        var service = new CleanupHostedService(
            new NoopActionDedupeCache(),
            new NoopFriendRequestThrottle(),
            new NoopDeviceLinkPollThrottle(),
            provider.GetRequiredService<IServiceScopeFactory>(),
            new GateOnlySessionEndAuthority(gate),
            NullLogger<CleanupHostedService>.Instance,
            TimeProvider.System);

        var sweep = service.SweepExpiredAccessOverridesAsync(
            now,
            TestContext.Current.CancellationToken);
        await sessions.FreshScopeReadStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        var dispatchCalls = 0;
        var actionAuthority = new ParticipantActionAuthority(
            session.Id,
            "expired-viewer",
            viewerId,
            "viewer-device",
            new ParticipantAccessDecision(
                IsOwnerParticipant: false,
                AccessLevel: AccessLevel.Suggest),
            runtime,
            queues,
            participantQueue,
            runtimes,
            registry,
            broadcaster,
            gate);
        var dispatching = actionAuthority.TryDispatchAsync(
                SessionCapability.Suggest,
                () =>
                {
                    Interlocked.Increment(ref dispatchCalls);
                    return broadcaster.SendToHost(session.Id, [1]);
                },
                TestContext.Current.CancellationToken)
            .AsTask();
        await WaitForReferenceCountAsync(gate, session.Id, expected: 2);
        Assert.False(dispatching.IsCompleted);

        sessions.AllowFreshScopeRead.TrySetResult();
        await sweep;
        var dispatch = await dispatching;

        Assert.NotNull(accessOverride.RevokedAt);
        var auditEntry = Assert.Single(audit.Entries);
        Assert.Equal(AccessOverrideAuditAction.Revoked, auditEntry.Action);
        Assert.Equal(AccessOverrideAuditReason.Expired, auditEntry.Reason);
        Assert.Equal(session.IncarnationId, auditEntry.SessionIncarnationId);
        Assert.Equal(session.StartedAt, auditEntry.SessionStartedAt);
        Assert.Equal(accessOverride.ExpiresAt, auditEntry.ExpectedExpiresAt);
        Assert.Equal(accessOverride.RevokedAt, auditEntry.ExpectedRevokedAt);
        Assert.Equal(now, auditEntry.OccurredAt);
        var work = Assert.Single(durability.Executed);
        Assert.Equal(auditEntry.Id, work.AuditEntryId);
        Assert.Equal(session.Id, work.SessionId);
        Assert.Equal(viewerId, work.GranteeUserId);
        Assert.Equal(session.IncarnationId, work.SessionIncarnationId);
        Assert.Equal(session.StartedAt, work.SessionStartedAt);
        Assert.Equal(accessOverride.ExpiresAt, work.ExpectedExpiresAt);
        Assert.Equal(accessOverride.RevokedAt, work.ExpectedRevokedAt);
        Assert.Equal([auditEntry.Id], durability.Completed);
        Assert.False(dispatch.Authorized);
        Assert.Equal(0, dispatchCalls);
        Assert.Equal(
            QueueCompletionCause.AccessRevokedCascade,
            participantQueue.CompletionCause);
    }

    [Fact]
    public async Task ExpiredAccess_Timeout_Releases_Gate_And_Leaves_Durable_Work_Pending()
    {
        var now = new DateTimeOffset(2026, 8, 20, 12, 0, 0, TimeSpan.Zero);
        var ownerId = UserId.New();
        var viewerId = UserId.New();
        var session = Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            1,
            Session.CurrentIncarnationProtocolVersion,
            ownerId,
            "expired-access-timeout",
            SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "secret");
        var accessOverride = SessionAccessOverride.Create(
            session.Id,
            viewerId,
            AccessLevel.Suggest,
            ownerId,
            expiresAt: now.AddMinutes(-1),
            createdAt: now.AddMinutes(-2));
        var audit = new FakeAccessOverrideAuditRepository();
        var durability = new BlockingAccessOverrideExpiryDurabilityCoordinator();
        var gate = new SessionLifecycleGate();
        var runtimes = new LiveSessionStateDirectory();
        var runtime = runtimes.CreateRuntime(session.Id);
        runtime.Host.SetSessionIncarnationId(session.IncarnationId);
        runtime.Host.SetSessionStartedAt(session.StartedAt);
        runtime.Host.SetStatus(SessionStatus.Live);
        var services = new ServiceCollection();
        services.AddLogging();
        services.AddSingleton<IAccessOverrideRepository>(
            new FakeAccessOverrideRepository(accessOverride));
        services.AddSingleton<IAccessOverrideAuditRepository>(audit);
        services.AddSingleton<ISessionRepository>(new FixedSessionRepository(session));
        services.AddSingleton<IUnitOfWork>(new FakeUnitOfWork());
        services.AddSingleton<IAccessOverrideExpiryDurabilityCoordinator>(durability);
        services.AddSingleton<IFriendshipRepository>(new FakeFriendshipRepository());
        services.AddSingleton<IRoomMemberRepository>(new FakeRoomMemberRepository());
        services.AddSingleton<ISessionViewerDismissalRepository>(
            new FakeSessionViewerDismissalRepository());
        services.AddSingleton<ILiveSessionStateDirectory>(runtimes);
        services.AddSingleton<IConnectionRegistry>(new ConnectionRegistry());
        services.AddSingleton<SessionBroadcaster>();
        services.AddSingleton<OperationalMetrics>();
        services.AddScoped<SessionAccessService>();
        services.AddScoped<SessionReader>();
        services.AddScoped<SessionAccessDisconnector>();
        await using var provider = services.BuildServiceProvider();
        var broadcaster = provider.GetRequiredService<SessionBroadcaster>();
        broadcaster.GetOrCreateSession(session.Id).SetHostQueue("host");
        var service = new CleanupHostedService(
            new NoopActionDedupeCache(),
            new NoopFriendRequestThrottle(),
            new NoopDeviceLinkPollThrottle(),
            provider.GetRequiredService<IServiceScopeFactory>(),
            new GateOnlySessionEndAuthority(gate),
            NullLogger<CleanupHostedService>.Instance,
            TimeProvider.System)
        {
            ImmediateEnforcementAttemptTimeout = TimeSpan.FromMilliseconds(20),
            LifecycleGateAttemptTimeout = TimeSpan.FromSeconds(5),
        };

        await service.SweepExpiredAccessOverridesAsync(
            now,
            TestContext.Current.CancellationToken);

        Assert.True(accessOverride.RevokedAt.HasValue);
        Assert.Single(audit.Entries);
        Assert.Empty(durability.Completed);
        Assert.Equal(0, gate.TestReferenceCount(session.Id));
        await using var reacquired = await gate.AcquireAsync(
            session.Id,
            TestContext.Current.CancellationToken);
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

    private sealed class RecordingAccessOverrideExpiryDurabilityCoordinator
        : IAccessOverrideExpiryDurabilityCoordinator
    {
        public List<Guid> Completed { get; } = [];
        public List<AccessOverrideExpiryEnforcementWork> Executed { get; } = [];

        public Task<IReadOnlyList<AccessOverrideExpiryEnforcementWork>> GetPendingEnforcementAsync(
            int limit,
            CancellationToken ct = default,
            AccessOverrideExpiryEnforcementCursor? before = null) => throw new NotSupportedException();

        public async Task<bool> ExecuteIfCurrentAsync(
            AccessOverrideExpiryEnforcementWork work,
            Func<CancellationToken, Task> enforce,
            CancellationToken ct = default)
        {
            Executed.Add(work);
            await enforce(ct);
            return true;
        }

        public Task CompleteEnforcementAsync(
            Guid auditEntryId,
            CancellationToken ct = default)
        {
            Completed.Add(auditEntryId);
            return Task.CompletedTask;
        }
    }

    private sealed class BlockingAccessOverrideExpiryDurabilityCoordinator
        : IAccessOverrideExpiryDurabilityCoordinator
    {
        public List<Guid> Completed { get; } = [];
        public Guid? AttemptedAuditEntryId { get; private set; }

        public Task<IReadOnlyList<AccessOverrideExpiryEnforcementWork>> GetPendingEnforcementAsync(
            int limit,
            CancellationToken ct = default,
            AccessOverrideExpiryEnforcementCursor? before = null) =>
            throw new NotSupportedException();

        public async Task<bool> ExecuteIfCurrentAsync(
            AccessOverrideExpiryEnforcementWork work,
            Func<CancellationToken, Task> enforce,
            CancellationToken ct = default)
        {
            AttemptedAuditEntryId = work.AuditEntryId;
            await Task.Delay(Timeout.InfiniteTimeSpan, ct);
            return true;
        }

        public Task CompleteEnforcementAsync(
            Guid auditEntryId,
            CancellationToken ct = default)
        {
            Completed.Add(auditEntryId);
            return Task.CompletedTask;
        }
    }

    private sealed class FixedSessionRepository(Session session) : SessionRepositoryStub
    {
        public override Task<Session?> GetByIdAsync(
            SessionId id,
            CancellationToken ct = default)
        {
            Assert.Equal(session.Id, id);
            return Task.FromResult<Session?>(session);
        }
    }

    private sealed class BlockingFreshScopeSessionRepository(Session session)
        : SessionRepositoryStub
    {
        private int _getByIdCalls;

        public TaskCompletionSource FreshScopeReadStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource AllowFreshScopeRead { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public override async Task<Session?> GetByIdAsync(
            SessionId id,
            CancellationToken ct = default)
        {
            Assert.Equal(session.Id, id);
            if (Interlocked.Increment(ref _getByIdCalls) == 2)
            {
                FreshScopeReadStarted.TrySetResult();
                await AllowFreshScopeRead.Task.WaitAsync(ct);
            }

            return session;
        }
    }

    private sealed class GateOnlySessionEndAuthority(
        SessionLifecycleGate gate) : ISessionEndAuthority
    {
        public ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default) =>
            gate.AcquireAsync(sessionIds, ct);

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

    private sealed class RecordingSemanticRelayRepository : ISemanticRelayRepository
    {
        public DateTimeOffset? Cutoff { get; private set; }
        public int? Limit { get; private set; }

        public Task<int> DeleteAcknowledgedBeforeAsync(
            DateTimeOffset cutoff,
            int limit,
            CancellationToken ct = default)
        {
            Cutoff = cutoff;
            Limit = limit;
            return Task.FromResult(0);
        }

        public Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsForDeviceAsync(
            UserId requesterUserId,
            string requesterDeviceId,
            int limit,
            SemanticReceiptCursor? cursor = null,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<SemanticRequestClaim> ClaimRequestAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<SemanticRequestClaim?> FindExactRequestAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task MarkDispatchedAsync(
            Guid requestRowId,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<bool> StoreReceiptAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            string outcome,
            UserId ownerUserId,
            string ownerDeviceId,
            string signature,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            int limit,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<bool> AcknowledgeReceiptAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            CancellationToken ct = default) => throw new NotSupportedException();
    }

    private sealed class NoopActionDedupeCache : IActionDedupeCache
    {
        public ActionDedupeClaim Claim(
            SessionId sessionId,
            UserId userId,
            string actionId,
            string? requestId = null) =>
            throw new NotSupportedException();

        public void Complete(
            SessionId sessionId,
            UserId userId,
            string actionId,
            long leaseId,
            ActionDedupeFinalOutcome outcome) =>
            throw new NotSupportedException();

        public void CompleteCurrent(
            SessionId sessionId,
            UserId userId,
            string actionId,
            ActionDedupeFinalOutcome outcome) =>
            throw new NotSupportedException();

        public bool TryClaimAuditSlot(
            SessionId sessionId,
            UserId userId,
            string actionId) =>
            throw new NotSupportedException();

        public void Sweep() { }
    }

    private sealed class NoopFriendRequestThrottle : IFriendRequestThrottle
    {
        public TimeSpan? TryClaim(UserId senderId, UserId targetId) =>
            throw new NotSupportedException();

        public void Release(UserId senderId, UserId targetId) =>
            throw new NotSupportedException();

        public void Sweep() { }
    }

    private sealed class NoopDeviceLinkPollThrottle : IDeviceLinkPollThrottle
    {
        public TimeSpan? TryClaim(string deviceCode) =>
            throw new NotSupportedException();

        public void Sweep() { }
    }
}
