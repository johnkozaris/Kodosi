using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;

namespace Kodosi.Host;

internal sealed class HeartbeatTimeoutHostedService(
    IServiceScopeFactory scopeFactory,
    ILiveSessionStateDirectory runtimes,
    IConnectionRegistry connections,
    SessionBroadcaster broadcaster,
    SessionEndCoordinator sessionEndCoordinator,
    SessionLifecycleGate lifecycleGate,
    OperationalMetrics metrics,
    TimeProvider timeProvider,
    ILogger<HeartbeatTimeoutHostedService> logger,
    RealtimePersistenceRepairTracker repairTracker) : BackgroundService
{
    private static readonly TimeSpan SweepInterval = TimeSpan.FromSeconds(5);
    private static readonly TimeSpan ReconnectThreshold = TimeSpan.FromSeconds(15);
    private static readonly TimeSpan EndThreshold = TimeSpan.FromSeconds(60);
    private static readonly TimeSpan DownstreamClientTimeout = TimeSpan.FromSeconds(120);

    private static readonly TimeSpan OrphanScavengerInterval = TimeSpan.FromSeconds(60);

    private static readonly TimeSpan OrphanGraceWindow = TimeSpan.FromMinutes(10);
    private DateTimeOffset _lastOrphanScavengeAt = DateTimeOffset.MinValue;

    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;
    private readonly IConnectionRegistry _connections = connections;
    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly SessionEndCoordinator _sessionEndCoordinator = sessionEndCoordinator;
    private readonly SessionLifecycleGate _lifecycleGate = lifecycleGate;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly TimeProvider _timeProvider = timeProvider;
    private readonly ILogger<HeartbeatTimeoutHostedService> _logger = logger;
    private readonly RealtimePersistenceRepairTracker _repairTracker = repairTracker;

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        while (!stoppingToken.IsCancellationRequested)
        {
            var sweepStart = _timeProvider.GetTimestamp();
            try
            {
                await SweepAsync(stoppingToken);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
            catch (Exception ex)
            {
                _logger.LogError(ex, "Error in heartbeat sweep");
            }
            finally
            {
                var elapsed = _timeProvider.GetElapsedTime(sweepStart).TotalMilliseconds;
                _metrics.RecordHeartbeatSweepDuration(elapsed);
            }

            try
            {
                await Task.Delay(SweepInterval, stoppingToken);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
        }
    }

    internal async Task SweepAsync(CancellationToken ct)
    {
        var now = _timeProvider.GetUtcNow();
        await RepairRealtimePersistenceAsync(ct);


        if (now - _lastOrphanScavengeAt >= OrphanScavengerInterval)
        {
            _lastOrphanScavengeAt = now;
            await SweepOrphanedLiveSessionsAsync(now - OrphanGraceWindow, ct);
        }

        foreach (var sessionId in _runtimes.GetActiveSessions())
        {
            var ports = _runtimes.TryGet(sessionId);
            if (ports is null) continue;

            await SweepStaleSharedParticipants(sessionId, ports.Participants, ports.Demand, DownstreamClientTimeout, ct);
            await SweepStaleOwnerParticipants(sessionId, ports.Participants, ports.Demand, DownstreamClientTimeout, ct);

            var expectedConnectionId = ports.Host.HostConnectionId;
            var expectedStartedAt = ports.Host.SessionStartedAt;
            if (expectedConnectionId is not null
                && expectedStartedAt.HasValue
                && ports.Host.IsHostHeartbeatPast(
                    expectedConnectionId,
                    ReconnectThreshold))
            {
                var transition =
                    await _sessionEndCoordinator
                        .TransitionTimedOutHostToReconnectingAsync(
                            sessionId,
                            ports,
                            expectedConnectionId,
                            expectedStartedAt.Value,
                            ReconnectThreshold,
                            ct);
                if (transition.Outcome == SessionTransitionOutcome.Applied)
                {
                    _metrics.RecordReconnect();
                    _logger.LogWarning("Session {SessionId} heartbeat timeout → Reconnecting", sessionId);
                    _metrics.RecordHostSlotTimeoutRelease();
                }
                else
                {
                    _logger.LogInformation(
                        "Skipped reconnecting broadcast for session {SessionId} because persistence outcome was {Outcome}",
                        sessionId,
                        transition.Outcome);
                }
            }
            else if (expectedStartedAt.HasValue
                && ports.Host.IsDisconnectedPast(EndThreshold))
            {
                var end = await _sessionEndCoordinator.EndDisconnectedAsync(
                    sessionId,
                    ports,
                    expectedStartedAt.Value,
                    EndThreshold,
                    CloseReason.SessionTimeout,
                    ct);
                if (end.Transition.Outcome != SessionTransitionOutcome.Applied)
                {
                    _logger.LogWarning(
                        "Session {SessionId} timed out in memory but persistence end outcome was {Outcome}",
                        sessionId,
                        end.Transition.Outcome);
                    continue;
                }

                _logger.LogWarning("Session {SessionId} timed out → Ended", sessionId);
                await ClearDbParticipantCountAsync(
                    sessionId,
                    expectedStartedAt,
                    ct);
            }
        }
    }

    internal Task SweepStaleSharedParticipants(
        SessionId sessionId,
        ILiveParticipantRoster participants,
        ILiveStreamDemand demand,
        TimeSpan timeout,
        CancellationToken ct = default)
    {
        return SweepStaleParticipants(
            sessionId,
            demand,
            participants.GetStaleSharedParticipantConnectionIds(timeout),
            participants.TryRemoveSharedParticipantIfStale,
            StreamDemandReason.SharedParticipantTimeout,
            decrementDbCount: true,
            timeout,
            ct);
    }

    internal Task SweepStaleOwnerParticipants(
        SessionId sessionId,
        ILiveParticipantRoster participants,
        ILiveStreamDemand demand,
        TimeSpan timeout,
        CancellationToken ct = default)
    {

        return SweepStaleParticipants(
            sessionId,
            demand,
            participants.GetStaleOwnerParticipantConnectionIds(timeout),
            participants.TryRemoveOwnerParticipantIfStale,
            StreamDemandReason.OwnerParticipantTimeout,
            decrementDbCount: false,
            timeout,
            ct);
    }

    private async Task SweepStaleParticipants(
        SessionId sessionId,
        ILiveStreamDemand demand,
        IReadOnlyList<string> staleConnectionIds,
        TryRemoveStaleParticipant removeParticipant,
        StreamDemandReason reason,
        bool decrementDbCount,
        TimeSpan timeout,
        CancellationToken ct)
    {
        if (staleConnectionIds.Count == 0) return;

        var runtime = _runtimes.TryGet(sessionId);
        var sessionQueues = _broadcaster.TryGetSession(sessionId);
        if (runtime is null || sessionQueues is null)
        {
            return;
        }
        await using var lifecycle = await _lifecycleGate.AcquireAsync(
            sessionId,
            ct);
        if (runtime is null
            || !ReferenceEquals(_runtimes.TryGet(sessionId), runtime)
            || !ReferenceEquals(runtime.Demand, demand))
        {
            return;
        }
        if (!ReferenceEquals(
                _broadcaster.TryGetSession(sessionId),
                sessionQueues))
        {
            return;
        }
        var sessionStartedAt = runtime.Host.SessionStartedAt;

        var activeParticipantsByConnectionId = _connections.GetActiveParticipants(sessionId)
            .ToDictionary(participant => participant.ConnectionId);

        var pendingDecrements = new List<Task>(staleConnectionIds.Count);
        foreach (var connectionId in staleConnectionIds)
        {
            var hasParticipant = activeParticipantsByConnectionId.TryGetValue(connectionId, out var participant);
            if (!removeParticipant(connectionId, timeout, out var transition))
            {
                continue;
            }
            _connections.Remove(connectionId);
            sessionQueues.GetParticipantQueue(connectionId)?.Complete(QueueCompletionCause.Timeout, discardPending: true);
            HostStreamDemandEmitter.TryEmit(
                _broadcaster,
                _metrics,
                _logger,
                sessionId,
                transition,
                reason);
            if (transition.Changed && hasParticipant)
            {
                _broadcaster.NotifyHostParticipantChanged(
                    sessionId,
                    demand.GetStreamDemand().ParticipantCount,
                    "left",
                    participant.UserId);
            }


            if (decrementDbCount && transition.Changed)
            {
                if (sessionStartedAt.HasValue)
                {
                    pendingDecrements.Add(DecrementDbParticipantAsync(
                        sessionId,
                        sessionStartedAt.Value,
                        runtime.IncarnationId,
                        ct));
                }
            }

            _logger.LogWarning(
                "Participant {ConnectionId} timed out ({Reason}, no activity for {Timeout}s) on session {SessionId}",
                connectionId,
                reason.ToWire(),
                DownstreamClientTimeout.TotalSeconds,
                sessionId);
        }

        if (pendingDecrements.Count > 0)
        {
            await Task.WhenAll(pendingDecrements);
        }
    }

    private delegate bool TryRemoveStaleParticipant(
        string connectionId,
        TimeSpan timeout,
        out StreamDemandTransition transition);

    private async Task DecrementDbParticipantAsync(
        SessionId sessionId,
        DateTimeOffset sessionStartedAt,
        Guid runtimeIncarnationId,
        CancellationToken ct)
    {
        try
        {
            using var scope = _scopeFactory.CreateScope();
            var repository = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
            _ = await repository.TryDecrementParticipantCountAsync(
                sessionId,
                sessionStartedAt,
                ct);
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
        }
        catch (Exception ex)
        {
            _repairTracker.MarkParticipantCount(
                sessionId,
                sessionStartedAt,
                runtimeIncarnationId);
            _metrics.RecordParticipantDecrementFailure();
            _logger.LogWarning(
                ex,
                "Failed to decrement DB participant count for session {SessionId} on stale sweep; queued reconciliation",
                sessionId);
        }
    }

    private async Task SweepOrphanedLiveSessionsAsync(DateTimeOffset noHostSince, CancellationToken ct)
    {
        try
        {
            using var scope = _scopeFactory.CreateScope();
            var repository = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
            var orphanIds = await repository.GetOrphanedLiveSessionsAsync(noHostSince, ct);
            if (orphanIds.Count == 0)
            {
                return;
            }

            _logger.LogWarning(
                "Orphan live-session scavenger found {Count} sessions without a host slot since {NoHostSince}",
                orphanIds.Count,
                noHostSince);

            foreach (var candidate in orphanIds)
            {
                var end = await _sessionEndCoordinator.EndOrphanedAsync(
                    candidate,
                    CloseReason.HostMissing,
                    ct);
                if (end.Transition.Outcome != SessionTransitionOutcome.Applied)
                {
                    _logger.LogWarning(
                        "Orphan scavenger could not end session {SessionId}: outcome {Outcome}",
                        candidate.SessionId,
                        end.Transition.Outcome);
                    continue;
                }
                _logger.LogWarning(
                    "Orphan scavenger ended session {SessionId} (no host for >= {Grace})",
                    candidate.SessionId,
                    OrphanGraceWindow);
                await ClearDbParticipantCountAsync(
                    candidate.SessionId,
                    candidate.StartedAt,
                    ct);
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Orphan live-session scavenger failed");
        }
    }

    private async Task RepairRealtimePersistenceAsync(CancellationToken ct)
    {
        var repairs = _repairTracker.Snapshot();
        if (repairs.HostReleases.Count == 0
            && repairs.ParticipantCounts.Count == 0)
        {
            return;
        }

        using var scope = _scopeFactory.CreateScope();
        var repository = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
        foreach (var repair in repairs.HostReleases)
        {
            try
            {
                await repository.ReleaseHostSlotAsync(
                    repair.SessionId,
                    repair.ConnectionId,
                    ct);
                _repairTracker.CompleteHostRelease(repair);
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
                throw;
            }
            catch (Exception ex)
            {
                _logger.LogWarning(
                    ex,
                    "Retry of DB host-slot release failed for session {SessionId}",
                    repair.SessionId);
            }
        }

        foreach (var repair in repairs.ParticipantCounts)
        {
            try
            {
                await using var lifecycle = await _lifecycleGate.AcquireAsync(
                    repair.SessionId,
                    ct);
                var runtime = _runtimes.TryGet(repair.SessionId);
                if (runtime is null)
                {
                    _ = await repository.TryClearParticipantCountAsync(
                        repair.SessionId,
                        repair.SessionStartedAt,
                        ct);
                    _repairTracker.CompleteParticipantCount(repair);
                    continue;
                }

                var currentStartedAt = runtime.Host.SessionStartedAt;
                if (!currentStartedAt.HasValue)
                {
                    continue;
                }
                if (currentStartedAt.Value != repair.SessionStartedAt)
                {


                    _repairTracker.CompleteParticipantCount(repair);
                    continue;
                }
                if (runtime.IncarnationId != repair.RuntimeIncarnationId)
                {
                    _repairTracker.CompleteParticipantCount(repair);
                    continue;
                }

                var participantCount =
                    runtime.Demand.GetStreamDemand().SharedParticipantCount;
                _ = await repository.TrySetParticipantCountAsync(
                    repair.SessionId,
                    repair.SessionStartedAt,
                    participantCount,
                    ct);
                _repairTracker.CompleteParticipantCount(repair);
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
                throw;
            }
            catch (Exception ex)
            {
                _logger.LogWarning(
                    ex,
                    "Retry of DB participant-count reconciliation failed for session {SessionId}",
                    repair.SessionId);
            }
        }
    }

    private async Task ClearDbParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset? expectedStartedAt,
        CancellationToken ct)
    {
        try
        {
            using var scope = _scopeFactory.CreateScope();
            var repository = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
            if (expectedStartedAt.HasValue)
            {
                _ = await repository.TryClearParticipantCountAsync(
                    sessionId,
                    expectedStartedAt.Value,
                    ct);
            }
            else
            {
                await repository.ClearParticipantCountAsync(sessionId, ct);
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {

        }
        catch (Exception ex)
        {
            _logger.LogWarning(
                ex,
                "Failed to clear DB participant count for session {SessionId} on session end",
                sessionId);
        }
    }

}
