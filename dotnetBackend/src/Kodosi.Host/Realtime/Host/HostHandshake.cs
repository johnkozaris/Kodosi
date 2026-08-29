using System.Net.WebSockets;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed class HostHandshake(
    IConnectionRegistry connections,
    ILiveSessionStateDirectory runtimes,
    SessionBroadcaster broadcaster,
    RealtimeDeviceAuthorizationReader deviceAuthorizationReader,
    IServiceScopeFactory scopeFactory,
    OperationalMetrics metrics,
    SessionLifecycleGate lifecycleGate,
    ILogger<HostHandshake> logger,
    TimeSpan? handshakeTimeout = null)
{
    private static readonly TimeSpan DefaultHandshakeTimeout = TimeSpan.FromSeconds(10);

    private readonly IConnectionRegistry _connections = connections;
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;
    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly RealtimeDeviceAuthorizationReader _deviceAuthorization =
        deviceAuthorizationReader;
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly SessionLifecycleGate _lifecycleGate = lifecycleGate;
    private readonly ILogger<HostHandshake> _logger = logger;
    private readonly TimeSpan _handshakeTimeout =
        handshakeTimeout ?? DefaultHandshakeTimeout;

    public async Task<bool> TryAcceptAsync(
        WebSocket webSocket,
        HostConnectionState state,
        CancellationToken ct)
    {
        if (!Guid.TryParse(state.SessionIdText, out var sessionGuid))
        {
            _logger.LogWarning("Invalid session ID format: {SessionId}", state.SessionIdText);
            _metrics.RecordHostWebSocketClose(CloseReason.InvalidSessionId);
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.InvalidPayloadData,
                CloseReason.InvalidSessionId.ToWire(),
                ct,
                _handshakeTimeout);
            return false;
        }

        var sessionId = SessionId.From(sessionGuid);
        state.SessionId = sessionId;

        if (!await RealtimeDeviceProof.SendChallengeAsync(
            webSocket,
            state.ConnectionId,
            "host",
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
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked.ToWire(),
                ct,
                _handshakeTimeout);
            return false;
        }

        var helloReadResult = await WebSocketHandshakeReader.ReceiveAsync(
            webSocket,
            RelayMessageLimits.GetMaxBytes("host.hello"),
            _handshakeTimeout,
            ct);
        if (helloReadResult is null)
        {
            _metrics.RecordHostWebSocketClose(CloseReason.ExpectedHostHello);
            if (webSocket.State == WebSocketState.Open)
            {
                await WebSocketCloseHelper.CloseOutputAsync(
                    webSocket,
                    WebSocketCloseStatus.PolicyViolation,
                    CloseReason.ExpectedHostHello.ToWire(),
                    ct);
            }
            return false;
        }
        if (helloReadResult.Value.Status == WebSocketMessageReader.ReadStatus.TooLarge)
        {
            webSocket.Abort();
            return false;
        }

        var helloResult = ParseHelloMessage(helloReadResult.Value);
        if (helloResult is null)
        {
            _metrics.RecordHostWebSocketClose(CloseReason.ExpectedHostHello);
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.ExpectedHostHello.ToWire(),
                ct,
                _handshakeTimeout);
            return false;
        }

        if (!string.Equals(proof.DeviceId, helloResult.DeviceId, StringComparison.Ordinal)
            || !string.Equals(
                helloResult.SessionId,
                sessionId.Value.ToString("D"),
                StringComparison.OrdinalIgnoreCase)
            || proof.ExpectedIncarnationId != helloResult.ExpectedIncarnationId)
        {
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked.ToWire(),
                ct,
                _handshakeTimeout);
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
                "host",
                sessionId.Value.ToString("D").ToLowerInvariant(),
                helloResult.ExpectedIncarnationId,
                deviceProofChallenge,
                ct)
            : null;
        state.DeviceProofChallenge = null;
        if (deviceAuthorization is null)
        {
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked.ToWire(),
                ct,
                _handshakeTimeout);
            return false;
        }
        var authorizedDevice = deviceAuthorization.Value;
        state.DeviceId = helloResult.DeviceId;
        state.StartDeviceAuthorizationLifetime(authorizedDevice.ExpiresAt);
        state.LinkedCts = CancellationTokenSource.CreateLinkedTokenSource(
            ct,
            state.DeviceAuthorizationCts.Token);

        if (!RelayProtocolVersions.IsAccepted(helloResult.RelayProtocolVersion))
        {
            _metrics.RecordHostWebSocketClose(CloseReason.UnsupportedData);
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.InvalidPayloadData,
                CloseReason.UnsupportedData.ToWire(),
                ct,
                _handshakeTimeout);
            return false;
        }
        const int relayProtocolVersion = RelayProtocolVersions.Current;
        var activationAttempt = await ActivateAsync(
            sessionId,
            state.AuthenticatedUserId,
            helloResult.SessionSecret,
            helloResult.ExpectedIncarnationId,
            state.ConnectionId,
            state.LinkedCts,
            state,
            ct);
        var activation = activationAttempt.Activation;
        if (activation.Outcome != HostSessionActivationOutcome.Applied)
        {
            await using var failedLifecycle = activationAttempt.Lifecycle;
            var closeReason =
                activation.Outcome == HostSessionActivationOutcome.AlreadyHosted
                    ? CloseReason.AlreadyHosted
                    : CloseReason.InvalidSessionOrSecret;
            _metrics.RecordHostWebSocketClose(closeReason);
            if (_runtimes.TryGet(sessionId) is null
                && _broadcaster.TryGetSession(sessionId) is { } staleQueues)
            {
                _broadcaster.RemoveSessionIfSame(
                    sessionId,
                    staleQueues,
                    closeReason);
            }
            await failedLifecycle.DisposeAsync();
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                closeReason.ToWire(),
                ct,
                _handshakeTimeout);
            return false;
        }
        await using var lifecycle = activationAttempt.Lifecycle;
        var ports = state.Ports!;
        _connections.RegisterHost(
            state.ConnectionId,
            state.AuthenticatedUserId,
            state.DeviceId,
            sessionId,
            state.DeviceAuthorizationCts);
        state.HostRegistered = true;



        authorizedDevice = await _deviceAuthorization.EvaluateAsync(
            state.AuthenticatedUserId,
            state.DeviceId,
            ct);
        if (state.DeviceAccessRevoked || !authorizedDevice.Authorized)
        {
            state.DiscardPreAcceptedState = true;
            DiscardOwnedPreAcceptedRuntime(
                sessionId,
                state,
                CloseReason.AccessRevoked);
            await lifecycle.DisposeAsync();
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked.ToWire(),
                ct,
                _handshakeTimeout);
            return false;
        }
        state.RescheduleDeviceAuthorizationExpiry(authorizedDevice.ExpiresAt);

        if (!activation.SessionStartedAt.HasValue
            || !state.HostClaimed
            || !ports.Host.HostConnected
            || !ReferenceEquals(_runtimes.TryGet(sessionId), ports))
        {
            state.DiscardPreAcceptedState = true;
            _metrics.RecordHostWebSocketClose(CloseReason.InvalidSessionState);
            DiscardOwnedPreAcceptedRuntime(
                sessionId,
                state,
                CloseReason.InvalidSessionState);
            await lifecycle.DisposeAsync();
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.InvalidSessionState.ToWire(),
                ct,
                _handshakeTimeout);
            return false;
        }

        state.SessionQueues = _broadcaster.GetOrCreateSession(sessionId);
        var accepted = new HostAcceptedMessage(
            state.SessionIdText,
            _broadcaster.RelayEpoch,
            activation.SessionIncarnationId!.Value,
            activation.SessionIncarnationGeneration!.Value,
            relayProtocolVersion);
        state.HostQueue = state.SessionQueues.TrySetHostQueue(
            state.ConnectionId,
            RelayOutbound.Encode(accepted));
        if (state.HostQueue is null)
        {
            state.DiscardPreAcceptedState = true;
            _metrics.RecordQueueOverflow(QueueOverflowLane.Host);
            DiscardOwnedPreAcceptedRuntime(
                sessionId,
                state,
                CloseReason.ServerError);
            await lifecycle.DisposeAsync();
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.InternalServerError,
                CloseReason.ServerError.ToWire(),
                ct,
                _handshakeTimeout);
            return false;
        }

        ports.Host.SetSessionStartedAt(activation.SessionStartedAt.Value);
        state.HostAccepted = true;
        state.PersistedLiveBeforeAcceptance = false;
        ports.Host.SetStatus(SessionStatus.Live);
        ports.Host.SetHostReady(true);
        _broadcaster.ReplayUnacknowledgedHostFences(sessionId);
        _broadcaster.FlushPendingHostFences(sessionId);
        _broadcaster.BroadcastSessionStatus(sessionId, SessionStatus.Live);
        TryQueueHostStreamDemand(
            sessionId,
            ports.Demand.GetStreamDemand(),
            StreamDemandReason.HostConnected);
        _logger.LogInformation(
            "Host connected: {ConnectionId} for session {SessionId}",
            state.ConnectionId,
            state.SessionIdText);
        return true;
    }

    private async Task<HostActivationAttempt> ActivateAsync(
        SessionId sessionId,
        UserId authenticatedUserId,
        string sessionSecret,
        Guid? expectedIncarnationId,
        string connectionId,
        CancellationTokenSource hostLifetime,
        HostConnectionState state,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var unitOfWork = scope.ServiceProvider.GetRequiredService<IUnitOfWork>();
        var userLifecycleLock =
            scope.ServiceProvider.GetRequiredService<IUserLifecycleLock>();
        var activator =
            scope.ServiceProvider.GetRequiredService<HostSessionActivator>();
        await using var transaction = await unitOfWork.BeginTransactionAsync(ct);
        await userLifecycleLock.AcquireAsync(authenticatedUserId, ct);
        var lifecycle = await _lifecycleGate.AcquireAsync(sessionId, ct);
        try
        {
            var activation =
                await activator.ActivateInsideExistingTransactionAsync(
                    sessionId,
                    authenticatedUserId,
                    sessionSecret,
                    expectedIncarnationId,
                    connectionId,
                    () =>
                    {
                        var claimed = _runtimes.TryClaimHost(
                            sessionId,
                            connectionId,
                            hostLifetime,
                            out var runtime,
                            out var creationOwnership);
                        if (claimed)
                        {
                            state.Ports = runtime;
                            state.RuntimeCreationOwnership = creationOwnership;
                            state.HostClaimed = true;



                            state.DurableSlotReleaseRequired = true;
                        }
                        return claimed;
                    },
                    ct);
            if (activation.Outcome != HostSessionActivationOutcome.Applied)
            {
                return new HostActivationAttempt(activation, lifecycle);
            }

            state.Ports?.Host.SetSessionIncarnationId(
                activation.SessionIncarnationId!.Value);
            state.ActivatedSessionStartedAt = activation.SessionStartedAt;
            state.ActivatedSessionIncarnationId = activation.SessionIncarnationId;
            state.ActivatedSessionIncarnationGeneration =
                activation.SessionIncarnationGeneration;
            state.ActivatedRuntimeIncarnationId = state.Ports?.IncarnationId;


            state.PersistedLiveBeforeAcceptance = true;
            await transaction.CommitAsync(ct);
            var sharedSurfaceEvents =
                scope.ServiceProvider.GetRequiredService<SharedSurfaceEventPublisher>();
            await SessionLifecycleInvalidationPublisher.TryPublishAsync(
                sharedSurfaceEvents,
                new LiveSessionTransitionResult(
                    SessionTransitionOutcome.Applied,
                    activation.SharingState,
                    activation.SessionStartedAt),
                _logger,
                ct);
            return new HostActivationAttempt(activation, lifecycle);
        }
        catch
        {
            state.DiscardPreAcceptedState = true;
            DiscardOwnedPreAcceptedRuntime(
                sessionId,
                state,
                CloseReason.ServerError);
            await lifecycle.DisposeAsync();
            throw;
        }
    }

    private void DiscardOwnedPreAcceptedRuntime(
        SessionId sessionId,
        HostConnectionState state,
        CloseReason closeReason)
    {
        if (state.Ports is not { } ports
            || state.RuntimeCreationOwnership is not { } creationOwnership)
        {
            return;
        }

        ports.Host.ReleaseHost(state.ConnectionId);
        if (!_runtimes.RemoveIfOwned(sessionId, creationOwnership))
        {
            return;
        }

        state.HostClaimed = false;
        state.RuntimeCreationOwnership = null;
        var queues = state.SessionQueues
            ?? _broadcaster.TryGetSession(sessionId);
        _broadcaster.RemoveSessionIfSame(
            sessionId,
            queues,
            closeReason);
    }

    private static HostHelloMessage? ParseHelloMessage(WebSocketMessageReader.Result readResult)
    {
        if (readResult.Status != WebSocketMessageReader.ReadStatus.Message
            || readResult.MessageType != WebSocketMessageType.Text)
        {
            return null;
        }

        var envelope = JsonSerializer.Deserialize(readResult.Payload, WsJsonContext.Default.WsEnvelope);
        if (envelope?.Type != "host.hello")
        {
            return null;
        }

        return JsonSerializer.Deserialize(readResult.Payload, WsJsonContext.Default.HostHelloMessage);
    }

    private void TryQueueHostStreamDemand(
        SessionId sessionId,
        StreamDemandSnapshot demand,
        StreamDemandReason reason)
    {
        var outcome = _broadcaster.SendHostStreamDemand(sessionId, demand, reason);
        if (outcome == ChannelByteSendQueueWriteOutcome.Full)
        {
            _metrics.RecordQueueOverflow(QueueOverflowLane.Host);
            _logger.LogWarning(
                "Host queue full while sending stream demand update for session {SessionId}",
                sessionId);
        }
    }

    internal sealed record HostActivationAttempt(
        HostSessionActivationResult Activation,
        IAsyncDisposable Lifecycle);
}
