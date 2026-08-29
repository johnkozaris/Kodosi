using System.Net.WebSockets;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed class ParticipantHandshake(
    IConnectionRegistry connections,
    ILiveSessionStateDirectory runtimes,
    RealtimeDeviceAuthorizationReader deviceAuthorizationReader,
    IServiceScopeFactory scopeFactory,
    SessionBroadcaster broadcaster,
    SessionReplaySender replaySender,
    OperationalMetrics metrics,
    SessionLifecycleGate lifecycleGate,
    ILogger<ParticipantHandshake> logger,
    RealtimePersistenceRepairTracker repairTracker,
    ISemanticRelayRepository semanticRelay,
    TimeSpan? handshakeTimeout = null)
{
    private const int MaxSharedParticipantsPerSession = 50;
    private const int MaxOwnerParticipantsPerSession = 8;
    private static readonly TimeSpan DefaultHandshakeTimeout = TimeSpan.FromSeconds(10);

    private readonly IConnectionRegistry _connections = connections;
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;
    private readonly RealtimeDeviceAuthorizationReader _deviceAuthorization =
        deviceAuthorizationReader;
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly SessionReplaySender _replaySender = replaySender;
    private readonly ISemanticRelayRepository _semanticRelay = semanticRelay;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly SessionLifecycleGate _lifecycleGate = lifecycleGate;
    private readonly ILogger<ParticipantHandshake> _logger = logger;
    private readonly TimeSpan _handshakeTimeout =
        handshakeTimeout ?? DefaultHandshakeTimeout;
    private readonly RealtimePersistenceRepairTracker _repairTracker = repairTracker;

    public async Task<bool> TryAcceptAsync(
        WebSocket webSocket,
        ParticipantConnectionState state,
        CancellationToken ct)
    {
        if (!Guid.TryParse(state.SessionIdText, out var sessionGuid))
        {
            _logger.LogWarning("Invalid session ID format: {SessionId}", state.SessionIdText);
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.InvalidPayloadData,
                CloseReason.InvalidSessionId,
                ct);
            return false;
        }

        var sessionId = SessionId.From(sessionGuid);
        state.SessionId = sessionId;

        if (!await RealtimeDeviceProof.SendChallengeAsync(
            webSocket,
            state.ConnectionId,
            "participant",
            sessionId.Value.ToString("D").ToLowerInvariant(),
            _handshakeTimeout,
            ct,
            challenge => state.DeviceProofChallenge = challenge))
        {
            return false;
        }

        var proofReadResult = await WebSocketHandshakeReader.ReceiveAsync(
            webSocket,
            RelayMessageLimits.GetMaxBytes("device.proof"),
            _handshakeTimeout,
            ct);
        if (proofReadResult is null
            || proofReadResult.Value.Status == WebSocketMessageReader.ReadStatus.TooLarge)
        {
            webSocket.Abort();
            return false;
        }
        var proof = JsonSerializer.Deserialize(
            proofReadResult.Value.Payload,
            WsJsonContext.Default.DeviceProofResponseMessage);
        if (proof?.Type != "device.proof"
            || proof.SessionId != sessionId.Value.ToString("D").ToLowerInvariant())
        {
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked,
                ct);
            return false;
        }

        var joinReadResult = await WebSocketHandshakeReader.ReceiveAsync(
            webSocket,
            RelayMessageLimits.GetMaxBytes("participant.join"),
            _handshakeTimeout,
            ct);
        if (joinReadResult is null)
        {
            if (webSocket.State == WebSocketState.Open)
            {
                await WebSocketCloseHelper.CloseOutputAsync(
                    webSocket,
                    WebSocketCloseStatus.PolicyViolation,
                    CloseReason.ExpectedParticipantJoin.ToWire(),
                    ct);
            }
            return false;
        }
        if (joinReadResult.Value.Status == WebSocketMessageReader.ReadStatus.TooLarge)
        {
            webSocket.Abort();
            return false;
        }

        var joinResult = ParseJoinMessage(joinReadResult.Value);
        if (joinResult is null)
        {
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.ExpectedParticipantJoin,
                ct);
            return false;
        }
        if (!string.Equals(proof.DeviceId, joinResult.DeviceId, StringComparison.Ordinal)
            || proof.ExpectedIncarnationId != joinResult.ExpectedIncarnationId)
        {
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked,
                ct);
            return false;
        }
        var deviceAuthorization = state.DeviceProofChallenge is { } deviceProofChallenge
            ? await RealtimeDeviceProof.VerifyAsync(
                _scopeFactory,
                _deviceAuthorization,
                state.AuthenticatedUserId,
                proof.DeviceId,
                proof.Signature,
                state.ConnectionId,
                "participant",
                sessionId.Value.ToString("D").ToLowerInvariant(),
                joinResult.ExpectedIncarnationId,
                deviceProofChallenge,
                ct)
            : null;
        state.DeviceProofChallenge = null;
        if (deviceAuthorization is null)
        {
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked,
                ct);
            return false;
        }
        state.DeviceId = joinResult.DeviceId;
        state.StartDeviceAuthorizationLifetime(deviceAuthorization.Value.ExpiresAt);

        var accessResolution = await ResolveAccessDecisionAsync(
            sessionId,
            state.AuthenticatedUserId,
            joinResult.ExpectedIncarnationId,
            ct);
        if (accessResolution is null)
        {
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.SessionNotFound,
                ct);
            return false;
        }
        if (!RelayProtocolVersions.IsAccepted(joinResult.RelayProtocolVersion))
        {
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.InvalidPayloadData,
                CloseReason.UnsupportedData,
                ct);
            return false;
        }
        const int relayProtocolVersion = RelayProtocolVersions.Current;
        var accessDecision = (ParticipantAccessDecision?)accessResolution.Value.Decision;
        if (!accessResolution.Value.AcceptsIncarnationHandshake)
        {
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.SessionNotLive,
                ct);
            return false;
        }
        state.AccessDecision = accessDecision.Value;

        var ports = _runtimes.TryGet(sessionId);
        if (ports is null)
        {
            var closeReason = accessDecision.Value.DurableSessionStatus is
                SessionStatus.Live or SessionStatus.Reconnecting
                    ? CloseReason.SessionRuntimeRecovering
                    : CloseReason.SessionNotLive;
            await CloseRejectedAsync(
                webSocket,
                closeReason.ToWebSocketCloseStatus(),
                closeReason,
                ct);
            return false;
        }
        if (ports.Host.Status == SessionStatus.Ended)
        {
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.SessionNotLive,
                ct);
            return false;
        }
        var currentReplayState = ports.Stream.GetReplayState();
        if (joinResult.LastSeenKeyGeneration > currentReplayState.CurrentKeyGeneration)
        {
            var closeReason = currentReplayState.CurrentKeyGeneration == 0
                ? CloseReason.SessionRuntimeRecovering
                : CloseReason.UnsupportedData;
            _logger.LogWarning(
                "Participant {ConnectionId} joined session {SessionId} with impossible future keyGeneration {ParticipantKeyGeneration}; backend is at {BackendKeyGeneration}",
                state.ConnectionId,
                state.SessionIdText,
                joinResult.LastSeenKeyGeneration,
                currentReplayState.CurrentKeyGeneration);
            await CloseRejectedAsync(
                webSocket,
                closeReason.ToWebSocketCloseStatus(),
                closeReason,
                ct);
            return false;
        }
        state.Ports = ports;

        await using var lifecycle = await _lifecycleGate.AcquireAsync(sessionId, ct);
        if (!ReferenceEquals(_runtimes.TryGet(sessionId), ports)
            || !await IsSessionIncarnationActiveAsync(
                sessionId,
                accessDecision.Value.SessionIncarnationId,
                ct))
        {
            await lifecycle.DisposeAsync();
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.SessionNotLive,
                ct);
            return false;
        }
        ports.Host.SetSessionStartedAt(accessDecision.Value.SessionStartedAt);

        var demandTransition = await ReserveParticipantAsync(
            state,
            sessionId,
            ports,
            accessDecision.Value,
            ct);
        if (demandTransition is null)
        {
            await lifecycle.DisposeAsync();
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.CapacityReached,
                ct);
            return false;
        }
        state.ParticipantReserved = true;

        state.LinkedCts = CancellationTokenSource.CreateLinkedTokenSource(
            ct,
            state.DeviceAuthorizationCts.Token);
        state.SessionQueues = _broadcaster.GetOrCreateSession(sessionId);
        var replayOutcome = SessionReplayQueueOutcome.Complete;
        state.ParticipantQueue = state.SessionQueues.AddPreparedParticipantQueue(
            state.ConnectionId,
            accessDecision.Value.AccessLevel,
            queue =>
            {
                var replayState = ports.Stream.GetReplayState();
                if (accessDecision.Value.IsOwnerParticipant)
                {
                    queue.MarkOwnerParticipant();
                }
                var accepted = new ParticipantAcceptedMessage(
                    state.SessionIdText,
                    accessDecision.Value.AccessLevel,
                    SessionCapabilities.FromAccess(
                        accessDecision.Value.AccessLevel,
                        accessDecision.Value.IsOwnerParticipant).ToMask(),
                    accessDecision.Value.SessionIncarnationId,
                    accessDecision.Value.SessionIncarnationGeneration,
                    relayProtocolVersion);
                var acceptedBytes = RelayOutbound.Encode(accepted);
                TryEnqueueControl(queue, WireMessage.Json(acceptedBytes));
                if (accessDecision.Value.IsOwnerParticipant
                    && !queue.BeginSemanticReceiptAdmission())
                {
                    replayOutcome = SessionReplayQueueOutcome.QueueFailure;
                    return;
                }
                if (ports.Host.Status != SessionStatus.Live)
                {
                    var statusBytes = RelayOutbound.Encode(new SessionStatusMessage(state.SessionIdText, ports.Host.Status));
                    TryEnqueueControl(queue, WireMessage.Json(statusBytes));
                }

                replayOutcome = _replaySender.QueueReplay(
                    replayState,
                    queue,
                    joinResult.CheckpointRevision,
                    joinResult.PresentationRevision,
                    joinResult.NextSequence,
                    joinResult.LastSeenKeyGeneration);
            },
            closeReason => WireMessage.Json(RelayOutbound.Encode(new SessionEndedMessage(
                state.SessionIdText,
                closeReason.ToWire()))),
            accessDecision.Value.IsOwnerParticipant
                ? new SessionSendQueues.SemanticReceiptDestination(
                    state.AuthenticatedUserId,
                    state.DeviceId!)
                : null);
        if (replayOutcome == SessionReplayQueueOutcome.QueueFailure)
        {
            _logger.LogWarning(
                "Participant {ConnectionId} for session {SessionId} could not admit its bounded replay",
                state.ConnectionId,
                state.SessionIdText);
            state.ParticipantQueue.Complete(
                QueueCompletionCause.Lagging,
                discardPending: true,
                closeReason: CloseReason.LaggingParticipant);
            await RemoveReservedParticipantAsync(
                state,
                accessDecision.Value,
                ports,
                sessionId);
            await lifecycle.DisposeAsync();
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.LaggingParticipant,
                ct);
            return false;
        }
        if (replayOutcome == SessionReplayQueueOutcome.TerminalGap)
        {
            _logger.LogWarning(
                "Participant {ConnectionId} for session {SessionId} requested an unavailable terminal replay boundary",
                state.ConnectionId,
                state.SessionIdText);
            state.ParticipantQueue.Complete(
                QueueCompletionCause.Lagging,
                discardPending: true,
                closeReason: CloseReason.TerminalReplayGap);
            await RemoveReservedParticipantAsync(
                state,
                accessDecision.Value,
                ports,
                sessionId);
            await lifecycle.DisposeAsync();
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.TerminalReplayGap,
                ct);
            return false;
        }
        RegisterConnection(state, accessDecision.Value, sessionId);
        state.ActionAuthority = new ParticipantActionAuthority(
            sessionId,
            state.ConnectionId,
            state.AuthenticatedUserId,
            state.DeviceId!,
            accessDecision.Value,
            ports,
            state.SessionQueues,
            state.ParticipantQueue,
            _runtimes,
            _connections,
            _broadcaster,
            _lifecycleGate);


        var finalDeviceAuthorization = await _deviceAuthorization.EvaluateAsync(
            state.AuthenticatedUserId,
            state.DeviceId,
            ct);
        if (state.DeviceAccessRevoked || !finalDeviceAuthorization.Authorized)
        {
            await lifecycle.DisposeAsync();
            await CloseRejectedAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked,
                ct);
            return false;
        }
        state.RescheduleDeviceAuthorizationExpiry(finalDeviceAuthorization.ExpiresAt);
        if (!await TryFinalizePublishedParticipantAsync(state, accessDecision.Value, ports, sessionId, ct))
        {
            state.ParticipantQueue.CancelSemanticReceiptAdmission();
            return true;
        }
        if (accessDecision.Value.IsOwnerParticipant)
        {
            var pendingSemanticReceipts = await _semanticRelay.ListPendingReceiptsAsync(
                sessionId,
                accessDecision.Value.SessionIncarnationId,
                state.AuthenticatedUserId,
                state.DeviceId!,
                limit: 1,
                ct);
            var receiptDeviceAuthorization = await _deviceAuthorization.EvaluateAsync(
                state.AuthenticatedUserId,
                state.DeviceId,
                ct);
            if (state.DeviceAccessRevoked || !receiptDeviceAuthorization.Authorized)
            {
                state.ParticipantQueue.CancelSemanticReceiptAdmission();
                await lifecycle.DisposeAsync();
                await CloseRejectedAsync(
                    webSocket,
                    WebSocketCloseStatus.PolicyViolation,
                    CloseReason.AccessRevoked,
                    ct);
                return false;
            }
            state.RescheduleDeviceAuthorizationExpiry(receiptDeviceAuthorization.ExpiresAt);
            if (!await TryFinalizePublishedParticipantAsync(
                    state,
                    accessDecision.Value,
                    ports,
                    sessionId,
                    ct))
            {
                state.ParticipantQueue.CancelSemanticReceiptAdmission();
                return true;
            }
            var receiptOutcome = state.ParticipantQueue.CompleteSemanticReceiptAdmission(
                pendingSemanticReceipts.Select(SemanticReceiptWire.Delivery).ToArray());
            if (receiptOutcome != RelayClientSendQueueWriteOutcome.Enqueued)
            {
                await RemoveReservedParticipantAsync(
                    state,
                    accessDecision.Value,
                    ports,
                    sessionId);
                return false;
            }
        }

        _logger.LogInformation(
            "Participant {ConnectionId} joined session {SessionId} with access {Access} (ownerParticipant={IsOwnerParticipant})",
            state.ConnectionId,
            state.SessionIdText,
            accessDecision.Value.AccessLevel,
            accessDecision.Value.IsOwnerParticipant);
        HostStreamDemandEmitter.TryEmit(
            _broadcaster,
            _metrics,
            _logger,
            sessionId,
            demandTransition,
            accessDecision.Value.IsOwnerParticipant
                ? StreamDemandReason.OwnerParticipantJoined
                : StreamDemandReason.SharedParticipantJoined);
        if (demandTransition.Changed)
        {
            _broadcaster.NotifyHostParticipantChanged(
                sessionId,
                ports.Demand.GetStreamDemand().ParticipantCount,
                "joined",
                state.AuthenticatedUserId);
        }

        return true;
    }

    private async Task<StreamDemandTransition?> ReserveParticipantAsync(
        ParticipantConnectionState state,
        SessionId sessionId,
        LiveSessionPorts ports,
        ParticipantAccessDecision accessDecision,
        CancellationToken ct)
    {
        if (accessDecision.IsOwnerParticipant)
        {
            if (ports.Participants.TryAddOwnerParticipant(
                    state.ConnectionId,
                    MaxOwnerParticipantsPerSession,
                    out var ownerTransition))
            {
                return ownerTransition;
            }

            _metrics.RecordCapacityRejected();
            return null;
        }

        if (!await TryIncrementParticipantCountAsync(
                sessionId,
                accessDecision.SessionStartedAt,
                MaxSharedParticipantsPerSession,
                ct))
        {
            _metrics.RecordCapacityRejected();
            return null;
        }

        state.DbParticipantCounted = true;

        if (!ports.Participants.TryAddSharedParticipant(
                state.ConnectionId,
                MaxSharedParticipantsPerSession,
                out var demandTransition))
        {
            _ = await TryDecrementParticipantCountAsync(
                sessionId,
                accessDecision.SessionStartedAt,
                ct);
            state.DbParticipantCounted = false;
            _metrics.RecordCapacityRejected();
            return null;
        }

        return demandTransition;
    }

    private void RegisterConnection(
        ParticipantConnectionState state,
        ParticipantAccessDecision accessDecision,
        SessionId sessionId)
    {
        if (accessDecision.IsOwnerParticipant)
        {
            _connections.RegisterOwnerParticipant(
                state.ConnectionId,
                state.AuthenticatedUserId,
                state.DeviceId!,
                sessionId,
                state.DeviceAuthorizationCts);
        }
        else
        {
            _connections.RegisterSharedParticipant(
                state.ConnectionId,
                state.AuthenticatedUserId,
                state.DeviceId!,
                sessionId,
                state.DeviceAuthorizationCts);
        }

        state.ParticipantRegistered = true;
    }

    private async Task<bool> TryFinalizePublishedParticipantAsync(
        ParticipantConnectionState state,
        ParticipantAccessDecision accessDecision,
        LiveSessionPorts ports,
        SessionId sessionId,
        CancellationToken ct)
    {
        var revalidatedResolution = await ResolveAccessDecisionAsync(
            sessionId,
            state.AuthenticatedUserId,
            accessDecision.SessionIncarnationId,
            ct);
        var revalidated = (ParticipantAccessDecision?)revalidatedResolution?.Decision;
        var runtimeStillPublished = ReferenceEquals(_runtimes.TryGet(sessionId), ports);
        var accessDecisionChanged = revalidated is { } currentAccessDecision && AccessDecisionChanged(accessDecision, currentAccessDecision);
        var terminalCloseReason = state.SessionQueues?.TerminalCloseReason;
        var liveStatus = ports.Host.Status;
        if (runtimeStillPublished
            && revalidated is not null
            && !accessDecisionChanged
            && liveStatus != SessionStatus.Ended
            && terminalCloseReason is null)
        {
            return true;
        }

        _logger.LogInformation(
            "Participant {ConnectionId} for session {SessionId}: access revoked or session ended during access-decision publish; liveStatus={LiveStatus}",
            state.ConnectionId,
            state.SessionIdText,
            liveStatus);

        if (accessDecisionChanged)
        {


            state.ParticipantQueue?.Complete(QueueCompletionCause.AccessRefresh, discardPending: true);
            await RemoveReservedParticipantAsync(state, accessDecision, ports, sessionId);
            return false;
        }

        var closeReason = terminalCloseReason
            ?? (liveStatus == SessionStatus.Ended ? CloseReason.SessionEnded : CloseReason.AccessRevoked);
        if (!runtimeStillPublished)
        {
            closeReason = CloseReason.SessionNotLive;
        }

        var completionCause = closeReason == CloseReason.AccessRevoked
            ? QueueCompletionCause.AccessRevokedCascade
            : QueueCompletionCause.SessionEnd;
        var message = closeReason == CloseReason.AccessRevoked
            ? WireMessage.Json(RelayOutbound.Encode(new SessionAccessRevokedMessage(state.SessionIdText)))
            : WireMessage.Json(RelayOutbound.Encode(new SessionEndedMessage(state.SessionIdText, closeReason.ToWire())));

        _ = state.ParticipantQueue?.TryCompleteWithControlMessage(message, completionCause, closeReason);
        await RemoveReservedParticipantAsync(
            state,
            accessDecision,
            ports,
            sessionId,
            removeSessionQueuesIfRuntimeUnpublished: !runtimeStillPublished);
        return false;
    }

    private static bool AccessDecisionChanged(ParticipantAccessDecision original, ParticipantAccessDecision current) =>
        original.IsOwnerParticipant != current.IsOwnerParticipant
        || original.AccessLevel != current.AccessLevel
        || original.SessionStartedAt != current.SessionStartedAt
        || original.SessionIncarnationId != current.SessionIncarnationId
        || original.SessionIncarnationGeneration != current.SessionIncarnationGeneration;

    private async Task RemoveReservedParticipantAsync(
        ParticipantConnectionState state,
        ParticipantAccessDecision accessDecision,
        LiveSessionPorts ports,
        SessionId sessionId,
        bool removeSessionQueuesIfRuntimeUnpublished = false)
    {
        if (state.ParticipantReserved && accessDecision.IsOwnerParticipant)
        {
            ports.Participants.RemoveOwnerParticipant(state.ConnectionId);
        }
        else if (state.ParticipantReserved)
        {
            ports.Participants.RemoveSharedParticipant(state.ConnectionId);
        }
        state.ParticipantReserved = false;

        if (state.ParticipantRegistered)
        {
            _connections.Remove(state.ConnectionId);
        }
        state.ParticipantRegistered = false;
        if (state.ParticipantQueue is { } participantQueue)
        {
            state.SessionQueues?.RemoveParticipantQueueIfSame(
                state.ConnectionId,
                participantQueue);
        }
        if (removeSessionQueuesIfRuntimeUnpublished && state.SessionQueues is { } sessionQueues)
        {
            _broadcaster.RemoveSessionIfSame(sessionId, sessionQueues, CloseReason.SessionNotLive);
        }

        if (!state.DbParticipantCounted)
        {
            return;
        }

        try
        {
            _ = await TryDecrementParticipantCountAsync(
                sessionId,
                accessDecision.SessionStartedAt,
                CancellationToken.None);
            state.DbParticipantCounted = false;
        }
        catch (Exception ex)
        {
            _repairTracker.MarkParticipantCount(
                sessionId,
                accessDecision.SessionStartedAt,
                ports.IncarnationId);
            _metrics.RecordParticipantDecrementFailure();
            _logger.LogWarning(
                ex,
                "Failed to decrement DB participant count for late-rejected session {SessionId}; HeartbeatTimeoutHostedService will reconcile on next sweep.",
                sessionId);
        }
    }

    private static ParticipantJoinMessage? ParseJoinMessage(WebSocketMessageReader.Result readResult)
    {
        if (readResult.Status != WebSocketMessageReader.ReadStatus.Message
            || readResult.MessageType != WebSocketMessageType.Text)
        {
            return null;
        }

        var envelope = JsonSerializer.Deserialize(readResult.Payload, WsJsonContext.Default.WsEnvelope);
        if (envelope?.Type != "participant.join")
        {
            return null;
        }

        try
        {
            return JsonSerializer.Deserialize(
                readResult.Payload,
                WsJsonContext.Default.ParticipantJoinMessage);
        }
        catch (JsonException)
        {
            return null;
        }
    }

    private async Task<ParticipantAccessResolution?> ResolveAccessDecisionAsync(
        SessionId sessionId,
        UserId authenticatedUserId,
        Guid? expectedIncarnationId,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var sessions = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
        var sessionAccess = scope.ServiceProvider.GetRequiredService<SessionAccessService>();
        var session = await sessions.GetByIdAsync(sessionId, ct);
        if (session is null || session.Status == SessionStatus.Ended)
        {
            return null;
        }

        var acceptsIncarnationHandshake =
            session.AcceptsIncarnationHandshake(expectedIncarnationId);

        if (session.IsOwner(authenticatedUserId))
        {
            return new ParticipantAccessResolution(
                new ParticipantAccessDecision(
                    true,
                    AccessLevel.Approve,
                    session.StartedAt,
                    session.IncarnationId,
                    session.IncarnationGeneration,
                    session.Status),
                acceptsIncarnationHandshake);
        }

        try
        {
            var accessLevel = await sessionAccess.ResolveAccessAsync(
                session,
                authenticatedUserId,
                ct);
            return new ParticipantAccessResolution(
                new ParticipantAccessDecision(
                    false,
                    accessLevel,
                    session.StartedAt,
                    session.IncarnationId,
                    session.IncarnationGeneration,
                    session.Status),
                acceptsIncarnationHandshake);
        }
        catch (PolicyViolationException)
        {
            return null;
        }
    }

    private async Task<bool> IsSessionIncarnationActiveAsync(
        SessionId sessionId,
        Guid expectedIncarnationId,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var sessions = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
        var session = await sessions.GetByIdAsync(sessionId, ct);
        return session is not null
            && session.Status != SessionStatus.Ended
            && session.IncarnationId == expectedIncarnationId;
    }

    private async Task<bool> TryIncrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        int maxParticipants,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var sessions = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
        return await sessions.TryIncrementParticipantCountAsync(
            sessionId,
            expectedStartedAt,
            maxParticipants,
            ct);
    }

    private async Task<bool> TryDecrementParticipantCountAsync(
        SessionId sessionId,
        DateTimeOffset expectedStartedAt,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var sessions = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
        return await sessions.TryDecrementParticipantCountAsync(
            sessionId,
            expectedStartedAt,
            ct);
    }

    private void TryEnqueueControl(RelayClientSendQueue sendQueue, WireMessage message)
    {
        if (sendQueue.TryEnqueueControl(message) == RelayClientSendQueueWriteOutcome.Overflow)
        {
            _metrics.RecordControlQueueOverflowDisconnect();
        }
    }

    private Task CloseRejectedAsync(
        WebSocket webSocket,
        WebSocketCloseStatus closeStatus,
        CloseReason closeReason,
        CancellationToken ct) =>
        WebSocketCloseHelper.CloseOutputAsync(
            webSocket,
            closeStatus,
            closeReason.ToWire(),
            ct,
            _handshakeTimeout);
}
