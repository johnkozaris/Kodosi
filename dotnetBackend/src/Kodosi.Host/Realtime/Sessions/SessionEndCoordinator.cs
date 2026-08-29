using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class SessionEndCoordinator(
    IServiceScopeFactory scopeFactory,
    ILiveSessionStateDirectory runtimes,
    SessionBroadcaster broadcaster,
    HostTeardown hostTeardown,
    SessionLifecycleGate lifecycleGate,
    ILogger<SessionEndCoordinator> logger) : ISessionEndAuthority
{
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;
    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly HostTeardown _hostTeardown = hostTeardown;
    private readonly SessionLifecycleGate _lifecycleGate = lifecycleGate;
    private readonly ILogger<SessionEndCoordinator> _logger = logger;

    public Task<SessionEndCoordinatorResult> EndOwnedIdempotentlyAsync(
        SessionId sessionId,
        UserId ownerUserId,
        Guid expectedIncarnationId,
        Guid mutationId,
        Guid attemptId,
        CloseReason closeReason,
        CancellationToken ct) =>
        EndAsync(
            sessionId,
            closeReason,
            ct,
            () => LiveSessionTransitions.EndOwnedSessionIdempotentlyAsync(
                _scopeFactory,
                _logger,
                sessionId,
                ownerUserId,
                expectedIncarnationId,
                mutationId,
                attemptId,
                ct));

    public Task<SessionEndCoordinatorResult> EndAuthenticatedHostAsync(
        SessionId sessionId,
        LiveSessionPorts expectedRuntime,
        string expectedConnectionId,
        Guid expectedRuntimeIncarnationId,
        CloseReason closeReason,
        CancellationToken ct) =>
        EndAuthenticatedHostCoreAsync(
            sessionId,
            expectedRuntime,
            expectedConnectionId,
            expectedRuntimeIncarnationId,
            closeReason,
            ct);

    private async Task<SessionEndCoordinatorResult>
        EndAuthenticatedHostCoreAsync(
            SessionId sessionId,
            LiveSessionPorts expectedRuntime,
            string expectedConnectionId,
            Guid expectedRuntimeIncarnationId,
            CloseReason closeReason,
            CancellationToken ct)
    {
        await using var lifecycle = await _lifecycleGate.AcquireAsync(sessionId, ct);
        var expectedStartedAt = expectedRuntime.Host.SessionStartedAt;
        if (!expectedStartedAt.HasValue
            || expectedRuntime.IncarnationId != expectedRuntimeIncarnationId
            || !ReferenceEquals(_runtimes.TryGet(sessionId), expectedRuntime)
            || !string.Equals(
                expectedRuntime.Host.HostConnectionId,
                expectedConnectionId,
                StringComparison.Ordinal))
        {
            return Rejected();
        }

        var transition = await LiveSessionTransitions.EndHostedSessionAsync(
            _scopeFactory,
            _logger,
            sessionId,
            expectedStartedAt.Value,
            expectedConnectionId,
            ct);
        if (!transition.Outcome.SatisfiesTargetState())
        {
            return new SessionEndCoordinatorResult(
                transition,
                ProjectionApplied: false);
        }

        var projectionApplied = ApplyRealtimeProjection(
            transition,
            closeReason,
            closeHost: false);
        return new SessionEndCoordinatorResult(
            transition,
            projectionApplied);
    }

    public async Task<SessionEndCoordinatorResult> EndDisconnectedAsync(
        SessionId sessionId,
        LiveSessionPorts expectedRuntime,
        DateTimeOffset expectedStartedAt,
        TimeSpan disconnectedFor,
        CloseReason closeReason,
        CancellationToken ct)
    {
        await using var lifecycle = await _lifecycleGate.AcquireAsync(sessionId, ct);
        if (!ReferenceEquals(_runtimes.TryGet(sessionId), expectedRuntime)
            || expectedRuntime.Host.SessionStartedAt != expectedStartedAt
            || expectedRuntime.Host.HostConnectionId is not null
            || !expectedRuntime.Host.IsDisconnectedPast(disconnectedFor))
        {
            return Rejected();
        }

        var transition = await LiveSessionTransitions.EndDisconnectedSessionAsync(
            _scopeFactory,
            _logger,
            sessionId,
            expectedStartedAt,
            ct);
        if (!transition.Outcome.SatisfiesTargetState())
        {
            return new SessionEndCoordinatorResult(
                transition,
                ProjectionApplied: false);
        }

        var projectionApplied = ApplyRealtimeProjection(
            transition,
            closeReason,
            closeHost: true);
        return new SessionEndCoordinatorResult(
            transition,
            projectionApplied);
    }

    public async Task<SessionEndCoordinatorResult> EndOrphanedAsync(
        OrphanedSessionCandidate candidate,
        CloseReason closeReason,
        CancellationToken ct)
    {
        await using var lifecycle = await _lifecycleGate.AcquireAsync(
            candidate.SessionId,
            ct);
        var transition = await LiveSessionTransitions.EndOrphanedSessionAsync(
            _scopeFactory,
            _logger,
            candidate,
            ct);
        if (!transition.Outcome.SatisfiesTargetState())
        {
            return new SessionEndCoordinatorResult(
                transition,
                ProjectionApplied: false);
        }

        var projectionApplied = ApplyRealtimeProjection(
            transition,
            closeReason,
            closeHost: true);
        return new SessionEndCoordinatorResult(
            transition,
            projectionApplied);
    }

    public async Task<LiveSessionTransitionResult>
        TransitionTimedOutHostToReconnectingAsync(
            SessionId sessionId,
            LiveSessionPorts expectedRuntime,
            string expectedConnectionId,
            DateTimeOffset expectedStartedAt,
            TimeSpan heartbeatTimeout,
            CancellationToken ct)
    {
        await using var lifecycle = await _lifecycleGate.AcquireAsync(sessionId, ct);
        if (!ReferenceEquals(_runtimes.TryGet(sessionId), expectedRuntime)
            || expectedRuntime.Host.SessionStartedAt != expectedStartedAt
            || !string.Equals(
                expectedRuntime.Host.HostConnectionId,
                expectedConnectionId,
                StringComparison.Ordinal)
            || !expectedRuntime.Host.IsHostHeartbeatPast(
                expectedConnectionId,
                heartbeatTimeout))
        {
            return Rejected().Transition;
        }

        var transition =
            await LiveSessionTransitions.TransitionTimedOutHostToReconnectingAsync(
                _scopeFactory,
                _logger,
                sessionId,
                expectedStartedAt,
                expectedConnectionId,
                ct);
        if (transition.Outcome != SessionTransitionOutcome.Applied)
        {
            return transition;
        }

        if (!expectedRuntime.Host.TryTimeoutHost(
                expectedConnectionId,
                heartbeatTimeout))
        {
            _logger.LogError(
                "Timed-out host {ConnectionId} for session {SessionId} changed after durable reconnect transition",
                expectedConnectionId,
                sessionId);
            return new LiveSessionTransitionResult(
                SessionTransitionOutcome.Rejected,
                transition.SharingState);
        }

        expectedRuntime.Host.SetStatus(SessionStatus.Reconnecting);
        _broadcaster.BroadcastSessionStatus(
            sessionId,
            SessionStatus.Reconnecting);
        return transition;
    }

    public ValueTask<IAsyncDisposable> AcquireAsync(
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct = default) =>
        _lifecycleGate.AcquireAsync(sessionIds, ct);

    public async Task ProjectCommittedAsync(
        IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
        CancellationToken ct = default)
    {
        if (sessionEnds.Count == 0)
        {
            return;
        }

        await using var scope = _scopeFactory.CreateAsyncScope();
        var sharedSurfaceEvents =
            scope.ServiceProvider.GetRequiredService<SharedSurfaceEventPublisher>();
        foreach (var sessionEnd in sessionEnds)
        {
            if (!sessionEnd.Transition.Outcome.SatisfiesTargetState()
                || sessionEnd.Transition.SharingState is null)
            {
                continue;
            }

            if (sessionEnd.Transition.Outcome == SessionTransitionOutcome.Applied)
            {
                await SessionLifecycleInvalidationPublisher.TryPublishAsync(
                    sharedSurfaceEvents,
                    sessionEnd.Transition,
                    _logger,
                    ct);
            }
            ApplyRealtimeProjection(
                sessionEnd.Transition,
                ToCloseReason(sessionEnd.Reason),
                closeHost: true);
        }
    }

    public Task RetireEndedIncarnationAsync(
        SessionDiscoveryTarget endedSession,
        DateTimeOffset startedAt,
        CancellationToken ct = default)
    {
        ct.ThrowIfCancellationRequested();
        ApplyRealtimeProjection(
            new LiveSessionTransitionResult(
                SessionTransitionOutcome.AlreadyInTargetState,
                endedSession,
                startedAt),
            CloseReason.SessionEnded,
            closeHost: true);
        return Task.CompletedTask;
    }

    private async Task<SessionEndCoordinatorResult> EndAsync(
        SessionId sessionId,
        CloseReason closeReason,
        CancellationToken ct,
        Func<Task<LiveSessionTransitionResult>> persistAsync)
    {
        await using var lifecycle = await _lifecycleGate.AcquireAsync(sessionId, ct);
        var transition = await persistAsync();
        if (!transition.Outcome.SatisfiesTargetState())
        {
            return new SessionEndCoordinatorResult(transition, ProjectionApplied: false);
        }

        var projectionApplied = ApplyRealtimeProjection(
            transition,
            closeReason,
            closeHost: true);

        return new SessionEndCoordinatorResult(transition, projectionApplied);
    }

    private bool ApplyRealtimeProjection(
        LiveSessionTransitionResult transition,
        CloseReason closeReason,
        bool closeHost)
    {
        if (transition.SharingState is not { } sharingState)
        {
            return false;
        }

        var sessionId = sharingState.SessionId;
        var runtime = _runtimes.TryGet(sessionId);
        var queues = _broadcaster.TryGetSession(sessionId);
        if (runtime is not null
            && sharingState.IncarnationId != Guid.Empty
            && runtime.Host.SessionIncarnationId != Guid.Empty
            && runtime.Host.SessionIncarnationId != sharingState.IncarnationId)
        {
            return false;
        }
        if (runtime is not null
            && transition.StartedAt.HasValue
            && runtime.Host.SessionStartedAt is { } runtimeStartedAt
            && runtimeStartedAt != transition.StartedAt)
        {
            return false;
        }
        if (runtime is not null)
        {
            runtime.Host.SetStatus(SessionStatus.Ended);
        }

        var applied = transition.Outcome == SessionTransitionOutcome.Applied;
        if (queues is not null)
        {



            _broadcaster.BroadcastSessionEnded(sessionId, closeReason);
        }
        if (!closeHost)
        {
            return applied && (runtime is not null || queues is not null);
        }

        _broadcaster.ForceDisconnectHost(sessionId, closeReason);
        _hostTeardown.CleanupEndedSession(
            sessionId,
            closeReason,
            runtime,
            queues);
        return applied && (runtime is not null || queues is not null);
    }

    private static CloseReason ToCloseReason(CommittedSessionEndReason reason) =>
        reason switch
        {
            CommittedSessionEndReason.OwnerIdentityReset =>
                CloseReason.OwnerIdentityReset,
            CommittedSessionEndReason.AccessRevoked =>
                CloseReason.AccessRevoked,
            _ => throw new ArgumentOutOfRangeException(
                nameof(reason),
                reason,
                "Unknown committed session end reason."),
        };

    private static SessionEndCoordinatorResult Rejected() =>
        new(
            new LiveSessionTransitionResult(
                SessionTransitionOutcome.Rejected,
                null),
            ProjectionApplied: false);
}

internal readonly record struct SessionEndCoordinatorResult(
    LiveSessionTransitionResult Transition,
    bool ProjectionApplied);
