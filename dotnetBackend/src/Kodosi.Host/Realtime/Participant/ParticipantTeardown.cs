using System.Net.WebSockets;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

namespace Kodosi.Host.Realtime;

internal sealed class ParticipantTeardown(
    IConnectionRegistry connections,
    ILiveSessionStateDirectory runtimes,
    IServiceScopeFactory scopeFactory,
    SessionBroadcaster broadcaster,
    OperationalMetrics metrics,
    SessionLifecycleGate lifecycleGate,
    ILogger<ParticipantTeardown> logger,
    RealtimePersistenceRepairTracker repairTracker)
{
    private readonly IConnectionRegistry _connections = connections;
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly SessionLifecycleGate _lifecycleGate = lifecycleGate;
    private readonly ILogger<ParticipantTeardown> _logger = logger;
    private readonly RealtimePersistenceRepairTracker _repairTracker = repairTracker;

    public async Task RunAsync(
        WebSocket webSocket,
        ParticipantConnectionState state,
        CancellationToken ct)
    {
        try
        {
            await CleanupRegistrationAsync(state);
            await CloseSocketAsync(webSocket, state, ct);
        }
        finally
        {
            state.LinkedCts?.Dispose();
            state.DisposeDeviceAuthorizationLifetime();
        }
    }

    private async Task CleanupRegistrationAsync(ParticipantConnectionState state)
    {
        if (!state.SessionId.HasValue)
        {
            return;
        }

        var sessionId = state.SessionId.Value;
        await using var lifecycle = await _lifecycleGate.AcquireAsync(
            sessionId,
            CancellationToken.None);
        StreamDemandTransition? disconnectTransition = null;
        var runtimeStillCurrent = state.Ports is not null
            && ReferenceEquals(
                _runtimes.TryGet(sessionId),
                state.Ports)
            && _runtimes.TryGet(sessionId)?.IncarnationId
                == state.Ports.IncarnationId;
        var queuesStillCurrent = state.SessionQueues is not null
            && ReferenceEquals(
                _broadcaster.TryGetSession(sessionId),
                state.SessionQueues);
        var wasRegistered = state.ParticipantRegistered;
        if ((state.ParticipantReserved || wasRegistered)
            && state.Ports is not null
            && state.AccessDecision is not null)
        {




            if (wasRegistered && runtimeStillCurrent && queuesStillCurrent)
            {
                _broadcaster.SendHostParticipantDisconnected(
                    sessionId,
                    state.ConnectionId);
            }

            var accessDecision = state.AccessDecision.Value;
            disconnectTransition = accessDecision.IsOwnerParticipant
                ? state.Ports.Participants.RemoveOwnerParticipant(state.ConnectionId)
                : state.Ports.Participants.RemoveSharedParticipant(state.ConnectionId);
            state.ParticipantReserved = false;
            if (state.ParticipantQueue is { } participantQueue)
            {
                state.SessionQueues?.RemoveParticipantQueueIfSame(
                    state.ConnectionId,
                    participantQueue);
            }
            if (wasRegistered)
            {
                _connections.Remove(state.ConnectionId);
            }
            state.ParticipantRegistered = false;
            if (wasRegistered && runtimeStillCurrent && queuesStillCurrent)
            {
                HostStreamDemandEmitter.TryEmit(
                    _broadcaster,
                    _metrics,
                    _logger,
                    sessionId,
                    disconnectTransition,
                    accessDecision.IsOwnerParticipant
                        ? StreamDemandReason.OwnerParticipantLeft
                        : StreamDemandReason.SharedParticipantLeft);
                if (disconnectTransition.Changed)
                {
                    _broadcaster.NotifyHostParticipantChanged(
                        sessionId,
                        state.Ports.Demand.GetStreamDemand().ParticipantCount,
                        "left",
                        state.AuthenticatedUserId);
                }
            }

            _logger.LogInformation(
                wasRegistered
                    ? "Participant disconnected: {ConnectionId}"
                    : "Rejected participant reservation cleaned: {ConnectionId}",
                state.ConnectionId);
        }

        if (state.DbParticipantCounted
            && state.AccessDecision is { } countedAccessDecision
            && (disconnectTransition?.Changed ?? true)
            && runtimeStillCurrent)
        {
            try
            {
                _ = await TryDecrementParticipantCountAsync(
                    sessionId,
                    countedAccessDecision.SessionStartedAt);
                state.DbParticipantCounted = false;
            }
            catch (Exception ex)
            {
                _repairTracker.MarkParticipantCount(
                    sessionId,
                    countedAccessDecision.SessionStartedAt,
                    state.Ports?.IncarnationId ?? Guid.Empty);
                _metrics.RecordParticipantDecrementFailure();
                _logger.LogWarning(
                    ex,
                    "Failed to decrement DB participant count for session {SessionId}; HeartbeatTimeoutHostedService will reconcile on next sweep.",
                    sessionId);
            }
        }
        else if (state.DbParticipantCounted && !runtimeStillCurrent)
        {
            state.DbParticipantCounted = false;
        }
    }

    private async Task<bool> TryDecrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var sessions = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
        return await sessions.TryDecrementParticipantCountAsync(
            sessionId,
            expectedStartedAt,
            CancellationToken.None);
    }

    private static Task CloseSocketAsync(
        WebSocket webSocket,
        ParticipantConnectionState state,
        CancellationToken ct)
    {
        if (webSocket.State is not (WebSocketState.Open or WebSocketState.CloseReceived))
        {
            return Task.CompletedTask;
        }

        var closeReason = state.AuthRevoked
            ? CloseReason.AuthRevoked
            : state.DeviceAccessRevoked
                ? CloseReason.AccessRevoked
            : state.ParticipantQueue?.CompletionCloseReason
                ?? state.ParticipantQueue?.CompletionCause?.ToCloseReason()
                ?? CloseReason.ClosingNormal;
        var closeStatus = closeReason.ToWebSocketCloseStatus();
        return WebSocketCloseHelper.CloseOutputAsync(
            webSocket,
            closeStatus,
            closeReason.ToWire(),
            ct);
    }
}
