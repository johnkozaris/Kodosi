using System.Net.WebSockets;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Host.Serialization;
using Kodosi.Infrastructure.Crypto;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class SessionEndCoordinatorTests
{
    [Theory]
    [InlineData("not-a-uuid")]
    [InlineData("00000000-0000-0000-0000-000000000000")]
    [InlineData("11111111-1111-4111-8111-111111111111")]
    public void SessionEndHeaders_RejectMalformedAndNonV7Values(string raw)
    {
        var context = new DefaultHttpContext();
        context.Request.Headers["Idempotency-Key"] = raw;

        Assert.Throws<InvalidParameterException>(() =>
            SessionEndpoints.RequiredUuidV7Header(context, "Idempotency-Key"));
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("   ")]
    public void SessionEndHeaders_RejectMissingValues(string? raw)
    {
        var context = new DefaultHttpContext();
        if (raw is not null)
        {
            context.Request.Headers["Idempotency-Key"] = raw;
        }

        Assert.Throws<MissingRequiredParameterException>(() =>
            SessionEndpoints.RequiredUuidV7Header(context, "Idempotency-Key"));
    }

    [Fact]
    public void SessionEndHeaders_AcceptUuidV7()
    {
        var context = new DefaultHttpContext();
        var value = Guid.CreateVersion7();
        context.Request.Headers["Kodosi-Attempt-Id"] = value.ToString("D");

        Assert.Equal(
            value,
            SessionEndpoints.RequiredUuidV7Header(
                context,
                "Kodosi-Attempt-Id"));
    }

    [Fact]
    public async Task DurableCommit_Completes_Before_Any_Realtime_Projection()
    {
        using var fixture = new Fixture(blockCommit: true);

        var ending = fixture.Coordinator.EndOwnedIdempotentlyAsync(
            fixture.Session.Id,
            fixture.Session.OwnerUserId,
            fixture.Session.IncarnationId,
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
            CloseReason.HostStopped,
            TestContext.Current.CancellationToken);
        await fixture.Store.CommitStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        Assert.False(ending.IsCompleted);
        Assert.False(fixture.Store.CommitCompleted);
        Assert.Equal(SessionStatus.Live, fixture.Runtime.Host.Status);
        Assert.Same(fixture.Runtime, fixture.Runtimes.TryGet(fixture.Session.Id));
        Assert.Null(fixture.HostQueue.CompletionReason);
        await AssertNoMessageAsync(fixture.ParticipantQueue);
        await AssertNoMessageAsync(fixture.OwnerEvents);

        fixture.Store.ReleaseCommit();
        var result = await ending;

        Assert.True(result.ProjectionApplied);
        Assert.True(fixture.Store.CommitCompleted);
        Assert.Equal(SessionStatus.Ended, fixture.Runtime.Host.Status);
        Assert.Null(fixture.Runtimes.TryGet(fixture.Session.Id));
    }

    [Fact]
    public async Task Teardown_Waits_For_PostCommit_Projection_And_Preserves_Requested_Reason()
    {
        using var fixture = new Fixture(blockInvalidation: true);
        using var hostLifetime = new CancellationTokenSource();
        Assert.True(fixture.Runtime.Host.TryClaimHost("host-1", hostLifetime));
        var state = new HostConnectionState(
            "host-1",
            fixture.Session.Id.Value.ToString(),
            fixture.Session.OwnerUserId)
        {
            SessionId = fixture.Session.Id,
            Ports = fixture.Runtime,
            SessionQueues = fixture.Queues,
            HostQueue = fixture.HostQueue,
            HostClaimed = true,
            HostAccepted = true,
        };
        using var webSocket = new RecordingWebSocket();

        var ending = fixture.Coordinator.EndOwnedIdempotentlyAsync(
            fixture.Session.Id,
            fixture.Session.OwnerUserId,
            fixture.Session.IncarnationId,
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
            CloseReason.HostStopped,
            TestContext.Current.CancellationToken);
        await fixture.BlockingFriendships!.ReadStarted.WaitAsync(
            TestContext.Current.CancellationToken);
        Assert.True(fixture.Store.CommitCompleted);

        var tearingDown = fixture.Teardown.RunAsync(
            webSocket,
            state,
            TestContext.Current.CancellationToken);
        await WaitForReferenceCountAsync(
            fixture.LifecycleGate,
            fixture.Session.Id,
            expected: 2);

        try
        {
            Assert.False(ending.IsCompleted);
            Assert.False(tearingDown.IsCompleted);
            Assert.Equal(SessionStatus.Live, fixture.Runtime.Host.Status);
            await AssertNoMessageAsync(fixture.ParticipantQueue);
        }
        finally
        {
            fixture.BlockingFriendships.Release();
        }

        var endResult = await ending;
        await tearingDown;

        Assert.True(endResult.ProjectionApplied);
        Assert.Equal(
            CloseReason.HostStopped.ToWire(),
            webSocket.LastCloseDescription);
        Assert.Equal(CloseReason.HostStopped, fixture.HostQueue.CompletionReason);
        Assert.Equal(0, fixture.LifecycleGate.TestActiveSessionCount());
        await fixture.AssertSingleEndedNotificationAsync();
    }

    [Fact]
    public async Task HttpOwner_And_AuthenticatedHost_Produce_Identical_Final_Outcomes()
    {
        using var http = new Fixture();
        using var host = new Fixture();

        var httpResult = await SessionEndpoints.EndOwnedIdempotentlyAsync(
            http.Session.Id.Value,
            http.Session.IncarnationId,
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            http.Coordinator,
            http.Session.OwnerUserId,
            TestContext.Current.CancellationToken);
        var hostResult = await host.EndAuthenticatedHostAsync(
            CloseReason.HostStopped,
            TestContext.Current.CancellationToken);
        host.Teardown.CleanupEndedSession(
            host.Session.Id,
            CloseReason.HostStopped,
            host.Runtime,
            host.Queues);

        Assert.Equal(
            StatusCodes.Status204NoContent,
            Assert.IsAssignableFrom<IStatusCodeHttpResult>(httpResult).StatusCode);
        Assert.True(hostResult.ProjectionApplied);
        Assert.Equal(await http.ReadOutcomeAsync(), await host.ReadOutcomeAsync());
    }

    [Fact]
    public async Task Concurrent_And_Repeated_End_Projects_Exactly_Once()
    {
        using var fixture = new Fixture(blockCommit: true);

        var hostEnd = fixture.EndAuthenticatedHostAsync(
            CloseReason.HostStopped,
            TestContext.Current.CancellationToken);
        await fixture.Store.CommitStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        var ownerEnd = fixture.Coordinator.EndOwnedIdempotentlyAsync(
            fixture.Session.Id,
            fixture.Session.OwnerUserId,
            fixture.Session.IncarnationId,
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
            CloseReason.HostStopped,
            TestContext.Current.CancellationToken);

        fixture.Store.ReleaseCommit();
        var concurrentResults = await Task.WhenAll(hostEnd, ownerEnd);
        fixture.Teardown.CleanupEndedSession(
            fixture.Session.Id,
            CloseReason.HostStopped,
            fixture.Runtime,
            fixture.Queues);

        var repeatedOwner = await fixture.Coordinator.EndOwnedIdempotentlyAsync(
            fixture.Session.Id,
            fixture.Session.OwnerUserId,
            fixture.Session.IncarnationId,
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
            CloseReason.HostStopped,
            TestContext.Current.CancellationToken);
        var repeatedHost = await fixture.EndAuthenticatedHostAsync(
            CloseReason.HostStopped,
            TestContext.Current.CancellationToken);

        Assert.Single(concurrentResults, result => result.ProjectionApplied);
        Assert.False(repeatedOwner.ProjectionApplied);
        Assert.False(repeatedHost.ProjectionApplied);
        Assert.Equal(1, fixture.Store.BlobDeleteCalls);
        Assert.Equal(SessionStatus.Ended, fixture.Runtime.Host.Status);
        Assert.Equal(CloseReason.HostStopped, fixture.HostQueue.CompletionReason);

        await fixture.AssertSingleNotificationsAsync();
    }

    [Fact]
    public async Task Delayed_Delete_For_Previous_Incarnation_Does_Not_End_Republished_Session()
    {
        using var fixture = new Fixture();
        var endedIncarnationId = fixture.Session.IncarnationId;
        fixture.Session.End();
        fixture.Session.Republish(Guid.CreateVersion7(), checked(fixture.Session.IncarnationGeneration + 1),
            fixture.Session.OwnerUserId,
            "republished",
            SessionScope.Friends,
            ToolKind.Terminal,
            AccessLevel.View,
            "new-secret",
            roomId: null);

        var result = await SessionEndpoints.EndOwnedIdempotentlyAsync(
            fixture.Session.Id.Value,
            endedIncarnationId,
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            fixture.Coordinator,
            fixture.Session.OwnerUserId,
            TestContext.Current.CancellationToken);

        Assert.Equal(
            StatusCodes.Status204NoContent,
            Assert.IsAssignableFrom<IStatusCodeHttpResult>(result).StatusCode);
        Assert.Equal(SessionStatus.Pending, fixture.Session.Status);
        Assert.NotEqual(endedIncarnationId, fixture.Session.IncarnationId);
        Assert.Equal(0, fixture.Store.BlobDeleteCalls);
    }

    [Fact]
    public async Task Timeout_And_OwnerEnd_Race_Projects_Exactly_Once()
    {
        using var fixture = new Fixture(blockCommit: true);
        fixture.Runtime.Host.SetSessionStartedAt(fixture.Session.StartedAt);
        fixture.Runtime.Host.TryClaimHost(
            "timed-out-host",
            new CancellationTokenSource());
        fixture.Runtime.Host.ReleaseHost("timed-out-host");

        var timeoutEnd = fixture.Coordinator.EndDisconnectedAsync(
            fixture.Session.Id,
            fixture.Runtime,
            fixture.Session.StartedAt,
            TimeSpan.Zero,
            CloseReason.SessionTimeout,
            TestContext.Current.CancellationToken);
        await fixture.Store.CommitStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);
        var ownerEnd = fixture.Coordinator.EndOwnedIdempotentlyAsync(
            fixture.Session.Id,
            fixture.Session.OwnerUserId,
            fixture.Session.IncarnationId,
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
            CloseReason.HostStopped,
            TestContext.Current.CancellationToken);

        fixture.Store.ReleaseCommit();
        var results = await Task.WhenAll(timeoutEnd, ownerEnd);

        Assert.True(results[0].ProjectionApplied);
        Assert.False(results[1].ProjectionApplied);
        Assert.Equal(1, fixture.Store.BlobDeleteCalls);
        Assert.Equal(CloseReason.SessionTimeout, fixture.HostQueue.CompletionReason);
        await fixture.AssertSingleNotificationsAsync("session_timeout");
    }

    [Fact]
    public async Task ReplayedOldRemovalEffectCannotRetireReplacementIncarnation()
    {
        using var fixture = new Fixture();
        fixture.Runtime.Host.SetSessionStartedAt(fixture.Session.StartedAt);
        fixture.Runtime.Host.SetSessionIncarnationId(fixture.Session.IncarnationId);
        var oldIncarnation = Guid.CreateVersion7();
        var oldTarget = new SessionDiscoveryTarget(
            fixture.Session.Id,
            fixture.Session.OwnerUserId,
            fixture.Session.Scope,
            fixture.Session.RoomId,
            oldIncarnation);

        await fixture.Coordinator.ProjectCommittedAsync(
            [new CommittedSessionEnd(
                new LiveSessionTransitionResult(
                    SessionTransitionOutcome.Applied,
                    oldTarget,
                    fixture.Session.StartedAt),
                CommittedSessionEndReason.AccessRevoked)],
            TestContext.Current.CancellationToken);

        Assert.Same(fixture.Runtime, fixture.Runtimes.TryGet(fixture.Session.Id));
        Assert.Same(fixture.Queues, fixture.Broadcaster.TryGetSession(fixture.Session.Id));
        Assert.Null(fixture.HostQueue.CompletionReason);
    }

    [Fact]
    public async Task Republish_Preparation_Retires_Ended_Runtime_And_Replaces_Queues()
    {
        using var fixture = new Fixture();
        fixture.Session.End();
        fixture.Runtime.Host.SetSessionStartedAt(fixture.Session.StartedAt);

        await using (await fixture.Coordinator.AcquireAsync(
            [fixture.Session.Id],
            TestContext.Current.CancellationToken))
        {
            await fixture.Coordinator.RetireEndedIncarnationAsync(
                fixture.Session.ToDiscoveryTarget(),
                fixture.Session.StartedAt,
                TestContext.Current.CancellationToken);
        }

        Assert.Null(fixture.Runtimes.TryGet(fixture.Session.Id));
        Assert.Null(fixture.Broadcaster.TryGetSession(fixture.Session.Id));
        Assert.Equal(
            QueueCompletionCause.SessionEnd,
            fixture.ParticipantQueue.CompletionCause);
        var replacementQueues =
            fixture.Broadcaster.GetOrCreateSession(fixture.Session.Id);
        Assert.NotSame(fixture.Queues, replacementQueues);
        var endedPayload = await fixture.ParticipantQueue.ReadAsync(
            TestContext.Current.CancellationToken);
        var ended = JsonSerializer.Deserialize(
            endedPayload!.Value.Payload,
            WsJsonContext.Default.SessionEndedMessage);
        Assert.Equal(CloseReason.SessionEnded.ToWire(), ended?.Reason);
        Assert.Null(await fixture.ParticipantQueue.ReadAsync(CancellationToken.None));
        await AssertNoMessageAsync(fixture.OwnerEvents);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task DurableFailure_Performs_No_Fanout(bool ownerHttp)
    {
        using var fixture = new Fixture(throwOnCommit: true);

        var ending = ownerHttp
            ? fixture.Coordinator.EndOwnedIdempotentlyAsync(
                fixture.Session.Id,
                fixture.Session.OwnerUserId,
                fixture.Session.IncarnationId,
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                CloseReason.HostStopped,
                TestContext.Current.CancellationToken)
            : fixture.EndAuthenticatedHostAsync(
                CloseReason.HostStopped,
                TestContext.Current.CancellationToken);

        await Assert.ThrowsAsync<InvalidOperationException>(() => ending);

        Assert.False(fixture.Store.CommitCompleted);
        Assert.Equal(SessionStatus.Live, fixture.Runtime.Host.Status);
        Assert.Same(fixture.Runtime, fixture.Runtimes.TryGet(fixture.Session.Id));
        Assert.Same(
            fixture.Queues,
            fixture.Broadcaster.TryGetSession(fixture.Session.Id));
        Assert.Null(fixture.HostQueue.CompletionReason);
        await AssertNoMessageAsync(fixture.ParticipantQueue);
        await AssertNoMessageAsync(fixture.OwnerEvents);
    }

    private static async Task AssertNoMessageAsync(RelayClientSendQueue queue)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(
            () => queue.ReadAsync(timeout.Token));
    }

    private static async Task AssertNoMessageAsync(ChannelByteSendQueue queue)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(25));
        await Assert.ThrowsAsync<OperationCanceledException>(
            () => queue.ReadAsync(timeout.Token));
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

    private sealed class Fixture : IDisposable
    {
        private readonly ServiceProvider _services;

        public Fixture(
            bool blockCommit = false,
            bool throwOnCommit = false,
            bool blockInvalidation = false)
        {
            Session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                UserId.New(),
                "Lifecycle authority",
                blockInvalidation ? SessionScope.Friends : SessionScope.JustMe,
                ToolKind.Terminal,
                AccessLevel.Suggest,
                new OwnerSessionSecretHasher().Hash("owner-secret"));
            Session.ActivateHost("test-host");
            Session.ReleaseHostSlot("test-host");
            Store = new EndStore(Session, blockCommit, throwOnCommit);
            Runtimes = new LiveSessionStateDirectory();
            Runtime = Runtimes.CreateRuntime(Session.Id);
            Runtime.Host.SetStatus(SessionStatus.Live);
            var metrics = new OperationalMetrics(Runtimes);
            Broadcaster = new SessionBroadcaster(
                Runtimes,
                metrics,
                NullLoggerFactory.Instance);
            Queues = Broadcaster.GetOrCreateSession(Session.Id);
            HostQueue = Queues.SetHostQueue("host-1");
            ParticipantQueue = Queues.AddParticipantQueue("participant-1");
            var userEvents = new UserEventBroadcaster(
                metrics,
                NullLogger<UserEventBroadcaster>.Instance);
            OwnerEvents = userEvents.Register(
                "owner-events",
                Session.OwnerUserId,
                "owner-device");
            BlockingFriendships = blockInvalidation
                ? new BlockingFriendshipRepository()
                : null;

            _services = new ServiceCollection()
                .AddScoped<ISessionRepository>(_ => new EndSessionRepository(Store))
                .AddScoped<ISessionKeyBlobRepository>(_ => new EndBlobRepository(Store))
                .AddScoped<ISessionEndMutationRepository>(
                    _ => new FakeSessionEndMutationRepository())
                .AddScoped<IUnitOfWork>(_ => new EndUnitOfWork(Store))
                .AddScoped<IFriendshipRepository>(
                    _ => BlockingFriendships is null
                        ? new FakeFriendshipRepository()
                        : BlockingFriendships)
                .AddScoped<IRoomMemberRepository>(_ => new FakeRoomMemberRepository())
                .AddScoped<ISessionViewerDismissalRepository>(
                    _ => new FakeSessionViewerDismissalRepository())
                .AddSingleton(userEvents)
                .AddScoped<DiscoveryAudienceResolver>()
                .AddScoped<SharedSurfaceEventPublisher>()
                .AddScoped<LiveSessionTerminator>()
                .BuildServiceProvider();
            var scopeFactory = _services.GetRequiredService<IServiceScopeFactory>();
            LifecycleGate = new SessionLifecycleGate();
            Teardown = new HostTeardown(
                new ConnectionRegistry(),
                Runtimes,
                Broadcaster,
                scopeFactory,
                metrics,
                LifecycleGate,
                NullLogger<HostTeardown>.Instance,
            repairTracker: new RealtimePersistenceRepairTracker(TimeProvider.System));
            Coordinator = new SessionEndCoordinator(
                scopeFactory,
                Runtimes,
                Broadcaster,
                Teardown,
                LifecycleGate,
                NullLogger<SessionEndCoordinator>.Instance);
        }

        public Session Session { get; }
        public EndStore Store { get; }
        public LiveSessionStateDirectory Runtimes { get; }
        public LiveSessionPorts Runtime { get; }
        public SessionBroadcaster Broadcaster { get; }
        public SessionSendQueues Queues { get; }
        public ChannelByteSendQueue HostQueue { get; }
        public RelayClientSendQueue ParticipantQueue { get; }
        public ChannelByteSendQueue OwnerEvents { get; }
        public BlockingFriendshipRepository? BlockingFriendships { get; }
        public SessionLifecycleGate LifecycleGate { get; }
        public HostTeardown Teardown { get; }
        public SessionEndCoordinator Coordinator { get; }

        public Task<SessionEndCoordinatorResult> EndAuthenticatedHostAsync(
            CloseReason closeReason,
            CancellationToken ct)
        {
            const string connectionId = "host-1";
            if (Session.HostConnectionSlot is null)
            {
                Session.ActivateHost(connectionId);
            }
            if (Runtime.Host.HostConnectionId is null)
            {
                Runtime.Host.TryClaimHost(
                    connectionId,
                    new CancellationTokenSource());
            }
            Runtime.Host.SetSessionStartedAt(Session.StartedAt);
            return Coordinator.EndAuthenticatedHostAsync(
                Session.Id,
                Runtime,
                connectionId,
                Runtime.IncarnationId,
                closeReason,
                ct);
        }

        public async Task<EndOutcome> ReadOutcomeAsync()
        {
            var endedPayload = await ParticipantQueue.ReadAsync(
                TestContext.Current.CancellationToken);
            var ended = JsonSerializer.Deserialize(
                endedPayload!.Value.Payload,
                WsJsonContext.Default.SessionEndedMessage);
            var invalidationPayload = await OwnerEvents.ReadAsync(
                TestContext.Current.CancellationToken);
            var invalidation = JsonSerializer.Deserialize(
                invalidationPayload!,
                WsJsonContext.Default.DiscoveryInvalidatedMessage);
            Assert.Equal(Session.Id.Value.ToString(), ended?.SessionId);

            return new EndOutcome(
                Session.Status,
                Store.BlobDeleteCalls,
                Store.CommitCompleted,
                Runtime.Host.Status,
                Runtimes.TryGet(Session.Id) is null,
                Broadcaster.TryGetSession(Session.Id) is null,
                HostQueue.CompletionReason,
                ParticipantQueue.CompletionCause,
                ParticipantQueue.CompletionCloseReason,
                ended?.Reason,
                string.Join(",", invalidation?.Surfaces ?? []));
        }

        public async Task AssertSingleNotificationsAsync(
            string expectedReason = "host_stopped")
        {
            var outcome = await ReadOutcomeAsync();
            Assert.Equal(SessionStatus.Ended, outcome.DurableStatus);
            Assert.Equal(expectedReason, outcome.EndReason);
            Assert.Equal(
                DiscoverySurface.OwnSessions.ToString(),
                outcome.InvalidationSurfaces);
            Assert.Null(await ParticipantQueue.ReadAsync(CancellationToken.None));
            await AssertNoMessageAsync(OwnerEvents);
        }

        public async Task AssertSingleEndedNotificationAsync()
        {
            var endedPayload = await ParticipantQueue.ReadAsync(
                TestContext.Current.CancellationToken);
            var ended = JsonSerializer.Deserialize(
                endedPayload!.Value.Payload,
                WsJsonContext.Default.SessionEndedMessage);
            Assert.Equal("host_stopped", ended?.Reason);
            Assert.Null(await ParticipantQueue.ReadAsync(CancellationToken.None));
        }

        public void Dispose()
        {
            Store.Dispose();
            _services.Dispose();
        }
    }

    private sealed record EndOutcome(
        SessionStatus DurableStatus,
        int BlobDeleteCalls,
        bool CommitCompleted,
        SessionStatus RuntimeStatus,
        bool RuntimeRemoved,
        bool QueuesRemoved,
        CloseReason? HostCloseReason,
        QueueCompletionCause? ParticipantCompletion,
        CloseReason? ParticipantCloseReason,
        string? EndReason,
        string InvalidationSurfaces);

    private sealed class EndStore(
        Session session,
        bool blockCommit,
        bool throwOnCommit) : IDisposable
    {
        private readonly SemaphoreSlim _transaction = new(1, 1);
        private readonly TaskCompletionSource _releaseCommit =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public Session Session { get; } = session;
        public bool BlockCommit { get; } = blockCommit;
        public bool ThrowOnCommit { get; } = throwOnCommit;
        public TaskCompletionSource CommitStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public bool CommitCompleted { get; set; }
        public int BlobDeleteCalls { get; set; }

        public async Task WaitForTransactionAsync(CancellationToken ct) =>
            await _transaction.WaitAsync(ct);

        public void ReleaseTransaction() => _transaction.Release();

        public Task WaitForCommitReleaseAsync(CancellationToken ct) =>
            _releaseCommit.Task.WaitAsync(ct);

        public void ReleaseCommit() => _releaseCommit.TrySetResult();

        public void Dispose() => _transaction.Dispose();
    }

    private sealed class EndSessionRepository(EndStore store) : SessionRepositoryStub
    {
        public override Task<Session?> GetByIdForUpdateAsync(
            SessionId id,
            CancellationToken ct = default) =>
            Task.FromResult<Session?>(store.Session.Id == id ? store.Session : null);

        public override Task UpdateAsync(
            Session session,
            CancellationToken ct = default) =>
            Task.CompletedTask;
    }

    private sealed class EndBlobRepository(EndStore store)
        : ISessionKeyBlobRepository
    {
        public Task DeleteForSessionAsync(
            SessionId sessionId,
            CancellationToken ct = default)
        {
            Assert.Equal(store.Session.Id, sessionId);
            store.BlobDeleteCalls++;
            return Task.CompletedTask;
        }

        public Task<SessionKeyBlob?> GetForDeviceAsync(
            SessionId sessionId,
            string recipientDeviceId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task AddRangeAsync(
            IReadOnlyList<SessionKeyBlob> blobs,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<DeviceRevocationSessionTarget>>
            GetSessionTargetsForRecipientDevicesAsync(
                IReadOnlyCollection<string> recipientDeviceIds,
                CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<SessionId>> GetSessionIdsForRecipientDevicesAsync(
            IReadOnlyCollection<string> recipientDeviceIds,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<int> DeleteForRecipientDevicesAsync(
            IReadOnlyCollection<string> recipientDeviceIds,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }

    private sealed class BlockingFriendshipRepository : IFriendshipRepository
    {
        private readonly TaskCompletionSource _readStarted =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        private readonly TaskCompletionSource _release =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public Task ReadStarted => _readStarted.Task;

        public void Release() => _release.TrySetResult();

        public Task<bool> AreFriendsAsync(
            UserId userA,
            UserId userB,
            CancellationToken ct = default) =>
            Task.FromResult(false);

        public async Task<IReadOnlyList<UserId>> GetFriendIdsAsync(
            UserId userId,
            CancellationToken ct = default)
        {
            _readStarted.TrySetResult();
            await _release.Task.WaitAsync(ct);
            return [];
        }

        public Task<IReadOnlyList<Friendship>> GetPendingIncomingAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<Friendship>> GetPendingOutgoingAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<Friendship?> GetAsync(
            UserId userA,
            UserId userB,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task AddAsync(
            Friendship friendship,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public void Remove(Friendship friendship) =>
            throw new NotSupportedException();
    }

    private sealed class EndUnitOfWork(EndStore store) : UnitOfWorkStub
    {
        public override Task SaveChangesAsync(CancellationToken ct = default) =>
            Task.CompletedTask;

        public override async Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default)
        {
            await store.WaitForTransactionAsync(ct);
            return new EndTransaction(store);
        }
    }

    private sealed class EndTransaction(EndStore store) : TransactionScopeStub
    {
        private bool _released;

        public override async Task CommitAsync(CancellationToken ct = default)
        {
            store.CommitStarted.TrySetResult();
            if (store.BlockCommit)
            {
                await store.WaitForCommitReleaseAsync(ct);
            }

            if (store.ThrowOnCommit)
            {
                Release();
                throw new InvalidOperationException("Commit failed.");
            }

            store.CommitCompleted = true;
            Release();
        }

        public override ValueTask DisposeAsync()
        {
            Release();
            return ValueTask.CompletedTask;
        }

        private void Release()
        {
            if (_released)
            {
                return;
            }

            _released = true;
            store.ReleaseTransaction();
        }
    }

    private sealed class RecordingWebSocket : WebSocket
    {
        public override WebSocketCloseStatus? CloseStatus => LastCloseStatus;
        public override string? CloseStatusDescription => LastCloseDescription;
        public override WebSocketState State => WebSocketState.Open;
        public override string? SubProtocol => null;
        public WebSocketCloseStatus? LastCloseStatus { get; private set; }
        public string? LastCloseDescription { get; private set; }

        public override void Abort() { }

        public override Task CloseAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken)
        {
            LastCloseStatus = closeStatus;
            LastCloseDescription = statusDescription;
            return Task.CompletedTask;
        }

        public override Task CloseOutputAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken)
        {
            LastCloseStatus = closeStatus;
            LastCloseDescription = statusDescription;
            return Task.CompletedTask;
        }

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
}
