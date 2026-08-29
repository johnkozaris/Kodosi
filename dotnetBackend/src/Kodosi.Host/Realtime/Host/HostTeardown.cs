using System.Net.WebSockets;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

namespace Kodosi.Host.Realtime;

internal sealed class HostTeardown(
    IConnectionRegistry connections,
    ILiveSessionStateDirectory runtimes,
    SessionBroadcaster broadcaster,
    IServiceScopeFactory scopeFactory,
    OperationalMetrics metrics,
    SessionLifecycleGate lifecycleGate,
    ILogger<HostTeardown> logger,
    RealtimePersistenceRepairTracker repairTracker)
{
    private readonly IConnectionRegistry _connections = connections;
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;
    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly SessionLifecycleGate _lifecycleGate = lifecycleGate;
    private readonly ILogger<HostTeardown> _logger = logger;
    private readonly RealtimePersistenceRepairTracker _repairTracker = repairTracker;

    public async Task RunAsync(
        WebSocket webSocket,
        HostConnectionState state,
        CancellationToken ct)
    {
        try
        {
            if (state.HostRegistered)
            {
                _connections.Remove(state.ConnectionId);
                state.HostRegistered = false;
            }

            if (state.HostClaimed)
            {
                try
                {
                    await CleanupRuntimeStateAsync(state);
                }
                finally
                {
                    state.Ports?.Host.ReleaseHost(state.ConnectionId);
                    state.HostClaimed = false;
                }
            }
            else if (state.PersistedLiveBeforeAcceptance
                && state.SessionId.HasValue)
            {
                await ReconcilePreAcceptedLiveAsync(state);
            }

            if (webSocket.State is WebSocketState.Open or WebSocketState.CloseReceived)
            {
                var closeReason = state.AuthRevoked
                    ? CloseReason.AuthRevoked
                    : state.DeviceAccessRevoked
                        ? CloseReason.AccessRevoked
                    : state.SessionQueues?.TerminalCloseReason
                        ?? state.CloseReason
                        ?? state.HostQueue?.CompletionReason
                        ?? CloseReason.ClosingNormal;
                _metrics.RecordHostWebSocketClose(closeReason);
                await WebSocketCloseHelper.CloseOutputAsync(
                    webSocket,
                    closeReason.ToWebSocketCloseStatus(),
                    closeReason.ToWire(),
                    ct);
            }
        }
        finally
        {
            if (state.DurableSlotReleaseRequired && state.SessionId.HasValue)
            {
                state.DurableSlotReleaseRequired = false;
                try
                {
                    await ReleaseHostSlotAsync(state.SessionId.Value, state.ConnectionId);
                }
                catch (Exception ex)
                {
                    _repairTracker.MarkHostRelease(
                        state.SessionId.Value,
                        state.ConnectionId);
                    _logger.LogWarning(
                        ex,
                        "Failed to release DB host slot for session {SessionId} connection {ConnectionId}; HeartbeatTimeoutHostedService will retry on its next sweep.",
                        state.SessionIdText,
                        state.ConnectionId);
                }
            }

            state.LinkedCts?.Dispose();
            state.DisposeDeviceAuthorizationLifetime();
        }
    }

    private async Task CleanupRuntimeStateAsync(HostConnectionState state)
    {
        var sessionId = state.SessionId!.Value;
        var ports = state.Ports!;
        await using var lifecycle = await _lifecycleGate.AcquireAsync(
            sessionId,
            CancellationToken.None);

        if (!state.HostAccepted)
        {
            var runtimeStillCurrent = ReferenceEquals(
                _runtimes.TryGet(sessionId),
                ports);
            var preLiveSessionQueues = state.SessionQueues
                ?? (runtimeStillCurrent
                    ? _broadcaster.TryGetSession(sessionId)
                    : null);
            var hasWaitingParticipants = ports.Demand.GetStreamDemand().ParticipantCount > 0
                || (preLiveSessionQueues?.ParticipantQueueCount ?? 0) > 0;
            var ownsRuntimeClaim = string.Equals(
                ports.Host.HostConnectionId,
                state.ConnectionId,
                StringComparison.Ordinal);
            ports.Host.ReleaseHost(state.ConnectionId);
            var removedRuntime = false;
            if (ownsRuntimeClaim
                && state.RuntimeCreationOwnership is { } creationOwnership
                && (state.DiscardPreAcceptedState || !hasWaitingParticipants))
            {
                removedRuntime = _runtimes.RemoveIfOwned(
                    sessionId,
                    creationOwnership);
                if (removedRuntime)
                {
                    state.RuntimeCreationOwnership = null;
                    _broadcaster.RemoveSessionIfSame(
                        sessionId,
                        preLiveSessionQueues);
                    _logger.LogInformation(
                        "Removed pre-live runtime for session {SessionId} after host connect failed before acceptance",
                        state.SessionIdText);
                }
            }

            if (!removedRuntime)
            {
                preLiveSessionQueues?.ClearHostQueue(state.ConnectionId);
                _logger.LogInformation(
                    "Preserved pre-live participant/session state for session {SessionId} after host connect failed before acceptance",
                    state.SessionIdText);
            }

            if (state.PersistedLiveBeforeAcceptance)
            {
                await ReconcilePreAcceptedLiveAsync(state);
            }
            return;
        }

        ports.Host.ReleaseHost(state.ConnectionId);


        var authoritativeEndReason = state.SessionQueues?.TerminalCloseReason;
        var effectiveCloseReason = authoritativeEndReason
            ?? state.CloseReason
            ?? state.HostQueue?.CompletionReason;
        state.SessionQueues?.ClearHostQueue(
            state.ConnectionId,
            effectiveCloseReason ?? CloseReason.ClosingNormal);

        if (state.HostEnded)
        {
            CleanupEndedSession(
                sessionId,
                effectiveCloseReason ?? CloseReason.SessionEnded,
                ports,
                state.SessionQueues);
            _logger.LogInformation("Host ended session {SessionId}, runtime cleaned", state.SessionIdText);
            return;
        }

        if (ports.Host.Status == SessionStatus.Ended)
        {
            CleanupEndedSession(
                sessionId,
                effectiveCloseReason ?? CloseReason.SessionEnded,
                ports,
                state.SessionQueues);
            _logger.LogInformation(
                "Host closed after session {SessionId} was ended by another entry path",
                state.SessionIdText);
            return;
        }

        if (ports.Host.HostConnected)
        {
            _logger.LogInformation(
                "Skipping reconnecting transition for session {SessionId} because a replacement host already claimed the runtime",
                state.SessionIdText);
            return;
        }

        if (ports.Host.Status == SessionStatus.Reconnecting)
        {
            _logger.LogInformation(
                "Skipping reconnecting transition for session {SessionId} because the runtime is already reconnecting",
                state.SessionIdText);
            return;
        }

        try
        {
            if (state.ActivatedSessionStartedAt is not { } expectedStartedAt
                || state.ActivatedSessionIncarnationId is not { } expectedIncarnationId)
            {
                _logger.LogError(
                    "Cannot transition disconnected host for session {SessionId}: activation fence is incomplete",
                    sessionId);
                return;
            }

            var reconnectTransition =
                await LiveSessionTransitions.TransitionExpectedHostToReconnectingAsync(
                    _scopeFactory,
                    _logger,
                    sessionId,
                    expectedStartedAt,
                    expectedIncarnationId,
                    state.ConnectionId,
                    CancellationToken.None);
            if (reconnectTransition.Outcome.SatisfiesTargetState())
            {
                ports.Host.SetStatus(SessionStatus.Reconnecting);
                _broadcaster.BroadcastSessionStatus(sessionId, SessionStatus.Reconnecting);
                _metrics.RecordReconnect();
                _logger.LogInformation(
                    "Host disconnected: {ConnectionId}, session {SessionId} now Reconnecting",
                    state.ConnectionId,
                    state.SessionIdText);
                return;
            }

            var persistedStatus = await GetPersistedStatusAsync(sessionId, CancellationToken.None);
            if (persistedStatus == SessionStatus.Ended)
            {
                ports.Host.SetStatus(SessionStatus.Ended);
                _broadcaster.BroadcastSessionEnded(sessionId, CloseReason.SessionEnded);
                CleanupEndedSession(
                    sessionId,
                    authoritativeEndReason ?? CloseReason.SessionEnded,
                    ports,
                    state.SessionQueues);
            }
            else if (persistedStatus.HasValue)
            {
                ports.Host.SetStatus(persistedStatus.Value);
                _broadcaster.BroadcastSessionStatus(
                    sessionId,
                    persistedStatus.Value);
            }

            _logger.LogInformation(
                "Skipping reconnecting broadcast for session {SessionId} because persistence outcome was {Outcome}",
                sessionId,
                reconnectTransition.Outcome);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Failed to transition session {SessionId} to Reconnecting", sessionId);
        }
    }

    internal void CleanupEndedSession(
        SessionId sessionId,
        CloseReason closeReason,
        LiveSessionPorts? expectedRuntime,
        SessionSendQueues? expectedQueues)
    {
        var removedRuntime = false;
        if (expectedRuntime is not null)
        {
            removedRuntime = _runtimes.RemoveIfSame(sessionId, expectedRuntime);
        }
        var removedQueues = _broadcaster.RemoveSessionIfSame(
            sessionId,
            expectedQueues,
            closeReason);
        if (removedRuntime || removedQueues)
        {
        }
    }

    private async Task<SessionStatus?> GetPersistedStatusAsync(
        SessionId sessionId,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var statusReader = scope.ServiceProvider.GetRequiredService<LiveSessionStatusReader>();
        return await statusReader.GetStatusAsync(sessionId, ct);
    }

    private async Task ReleaseHostSlotAsync(
        SessionId sessionId,
        string connectionId)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var repository = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
        await repository.ReleaseHostSlotAsync(sessionId, connectionId, CancellationToken.None);
    }

    private async Task ReconcilePreAcceptedLiveAsync(HostConnectionState state)
    {
        if (state.SessionId is not { } sessionId
            || state.ActivatedSessionStartedAt is not { } expectedStartedAt
            || state.ActivatedSessionIncarnationId is not { } expectedIncarnationId
            || state.ActivatedRuntimeIncarnationId is not { } expectedRuntimeIncarnationId)
        {
            _logger.LogWarning(
                "Could not reconcile pre-accepted live session {SessionId}: activation fence is incomplete",
                state.SessionIdText);
            return;
        }

        try
        {
            var transition =
                await LiveSessionTransitions.TransitionExpectedHostToReconnectingAsync(
                _scopeFactory,
                _logger,
                sessionId,
                expectedStartedAt,
                expectedIncarnationId,
                state.ConnectionId,
                CancellationToken.None);
            if (!transition.Outcome.SatisfiesTargetState())
            {
                _logger.LogWarning(
                    "Could not reconcile pre-accepted live session {SessionId}: {Outcome}",
                    sessionId,
                    transition.Outcome);
                return;
            }

            if (state.Ports is { } expectedPorts
                && expectedPorts.IncarnationId == expectedRuntimeIncarnationId
                && ReferenceEquals(_runtimes.TryGet(sessionId), expectedPorts))
            {
                expectedPorts.Host.SetStatus(SessionStatus.Reconnecting);
                _broadcaster.BroadcastSessionStatus(sessionId, SessionStatus.Reconnecting);
            }
        }
        catch (Exception ex)
        {
            _logger.LogError(
                ex,
                "Failed to reconcile pre-accepted live session {SessionId}",
                sessionId);
        }
    }
}
