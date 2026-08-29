using System.Net.WebSockets;
using System.Text.Json;
using Microsoft.Extensions.DependencyInjection;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Auth;
using Kodosi.Host.Observability;
using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed class UserEventsWebSocketHandler(
    UserEventBroadcaster broadcaster,
    RealtimeDeviceAuthorizationReader deviceAuthorization,
    IJwtRevalidator jwtRevalidator,
    IServiceScopeFactory scopeFactory,
    OperationalMetrics metrics,
    ILogger<UserEventsWebSocketHandler> logger)
{
    private const int InboundMaxMessageBytes = 4 * 1024;

    private readonly UserEventBroadcaster _broadcaster = broadcaster;
    private readonly RealtimeDeviceAuthorizationReader _deviceAuthorization =
        deviceAuthorization;
    private readonly IJwtRevalidator _jwtRevalidator = jwtRevalidator;
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly ILogger<UserEventsWebSocketHandler> _logger = logger;

    public async Task HandleAsync(
        WebSocket webSocket,
        UserId authenticatedUserId,
        string? accessToken,
        CancellationToken ct)
    {
        var connectionId = Guid.NewGuid().ToString();
        byte[]? challenge = null;
        if (!await RealtimeDeviceProof.SendChallengeAsync(
            webSocket,
            connectionId,
            "user-events",
            null,
            TimeSpan.FromSeconds(10),
            ct,
            value => challenge = value))
        {
            return;
        }
        var proofRead = await WebSocketHandshakeReader.ReceiveAsync(
            webSocket,
            RelayMessageLimits.GetMaxBytes("device.proof"),
            TimeSpan.FromSeconds(10),
            ct);
        if (proofRead is null
            || proofRead.Value.Status == WebSocketMessageReader.ReadStatus.TooLarge)
        {
            webSocket.Abort();
            return;
        }
        var proof = JsonSerializer.Deserialize(
            proofRead.Value.Payload,
            WsJsonContext.Default.DeviceProofResponseMessage);
        var initialAuthorization = proof is { Type: "device.proof" }
            && challenge is not null
            && proof.SessionId is null
            && proof.ExpectedIncarnationId is null
            ? await RealtimeDeviceProof.VerifyAsync(
                _scopeFactory,
                _deviceAuthorization,
                authenticatedUserId,
                proof.DeviceId,
                proof.Signature,
                connectionId,
                "user-events",
                null,
                null,
                challenge,
                ct)
            : null;
        if (initialAuthorization is null)
        {
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked.ToWire(),
                ct);
            return;
        }
        var deviceId = proof!.DeviceId;

        var registration = _broadcaster.RegisterPrimed(
            connectionId,
            authenticatedUserId,
            deviceId);
        var sendQueue = registration.Queue;
        if (sendQueue.CompletionReason == CloseReason.AccessRevoked)
        {
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked.ToWire(),
                ct);
            return;
        }
        try
        {
            await SendDurableWithdrawalsAsync(
                registration,
                webSocket,
                authenticatedUserId,
                ct);
            await SendDeviceLinkSnapshotAsync(
                registration,
                webSocket,
                authenticatedUserId,
                ct);
            if (sendQueue.CompletionReason == CloseReason.AccessRevoked)
            {
                _broadcaster.Remove(connectionId);
                await WebSocketCloseHelper.CloseOutputAsync(
                    webSocket,
                    WebSocketCloseStatus.PolicyViolation,
                    CloseReason.AccessRevoked.ToWire(),
                    ct);
                return;
            }
            if (registration.CompletePriming()
                != ChannelByteSendQueueWriteOutcome.Enqueued)
            {
                throw new InvalidOperationException(
                    "Live user events exceeded the queue while durable replay was loading.");
            }
        }
        catch
        {
            sendQueue.Complete(CloseReason.ServerError, discardPending: true);
            _broadcaster.Remove(connectionId);
            throw;
        }
        ActiveDeviceAuthorizationDecision finalAuthorization;
        try
        {
            finalAuthorization = await _deviceAuthorization.EvaluateAsync(
                authenticatedUserId,
                deviceId,
                ct);
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            _broadcaster.Remove(connectionId);
            return;
        }
        catch
        {
            _broadcaster.Remove(connectionId);
            throw;
        }
        if (!finalAuthorization.Authorized
            || sendQueue.CompletionReason == CloseReason.AccessRevoked)
        {
            _broadcaster.Remove(connectionId);
            await WebSocketCloseHelper.CloseOutputAsync(
                webSocket,
                WebSocketCloseStatus.PolicyViolation,
                CloseReason.AccessRevoked.ToWire(),
                ct);
            return;
        }

        using var linkedCts = CancellationTokenSource.CreateLinkedTokenSource(
            ct,
            sendQueue.CompletionToken);
        var linkedToken = linkedCts.Token;
        var authorizationFailureReason = (CloseReason?)null;

        try
        {
            var sendTask = SendPumpAsync(webSocket, sendQueue, linkedToken);
            var receiveTask = ReceivePumpAsync(webSocket, linkedToken);
            var revalidateTask = accessToken is null
                ? Task.CompletedTask
                : RevalidateAuthorizationPeriodicallyAsync(
                    accessToken,
                    authenticatedUserId,
                    deviceId,
                    connectionId,
                    reason => { authorizationFailureReason ??= reason; },
                    linkedCts,
                    linkedToken);

            await Task.WhenAny(sendTask, receiveTask, revalidateTask);

            await linkedCts.CancelAsync();
            try
            {
                await Task.WhenAll(sendTask, receiveTask, revalidateTask);
            }
            catch (OperationCanceledException)
            {
            }
        }
        catch (OperationCanceledException)
        {
        }
        catch (WebSocketException ex)
        {
            _logger.LogWarning(ex, "User events websocket error for {ConnectionId}", connectionId);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Unhandled user events websocket failure for {ConnectionId}", connectionId);
        }
        finally
        {
            _broadcaster.Remove(connectionId);

            if (webSocket.State is WebSocketState.Open or WebSocketState.CloseReceived)
            {
                var closeReason = authorizationFailureReason
                    ?? sendQueue.CompletionReason
                    ?? CloseReason.ClosingNormal;
                await WebSocketCloseHelper.CloseOutputAsync(
                    webSocket,
                    closeReason.ToWebSocketCloseStatus(),
                    closeReason.ToWire(),
                    ct);
            }
        }
    }

    private async Task SendDeviceLinkSnapshotAsync(
        UserEventBroadcaster.UserEventRegistration registration,
        WebSocket webSocket,
        UserId recipientUserId,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var repository = scope.ServiceProvider
            .GetRequiredService<IDeviceLinkRequestRepository>();
        var pending = await repository.ListPendingForUserAsync(
            recipientUserId,
            DateTimeOffset.UtcNow,
            ct);
        var payload = RelayOutbound.Encode(new UserDeviceLinkSnapshotMessage(
            pending
                .Select(request => new UserDeviceLinkSnapshotEntry(
                    request.UserCode,
                    request.DeviceLabel,
                    request.ExpiresAt))
                .ToArray()));
        if (!await WebSocketSendDeadline.SendAsync(
                webSocket,
                payload,
                WebSocketMessageType.Text,
                TimeSpan.FromSeconds(5),
                ct,
                throwOnCancellation: false))
        {
            throw new WebSocketException("Durable device-link snapshot could not be delivered.");
        }
        if (registration.Queue.CompletionReason == CloseReason.AccessRevoked)
        {
            return;
        }
    }

    private async Task SendDurableWithdrawalsAsync(
        UserEventBroadcaster.UserEventRegistration registration,
        WebSocket webSocket,
        UserId recipientUserId,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var repository = scope.ServiceProvider
            .GetRequiredService<IIdentityExposureRepository>();
        var lifecycleSnapshot = await repository.GetLifecycleSnapshotForRecipientAsync(
            recipientUserId,
            ct);
        foreach (var lifecycle in lifecycleSnapshot)
        {
            if (registration.Queue.CompletionReason == CloseReason.AccessRevoked)
            {
                return;
            }
            var state = lifecycle.IdentityIncarnationId.HasValue ? "enrolled" : "withdrawn";
            var payload = RelayOutbound.Encode(new UserIdentityLifecycleChangedMessage(
                lifecycle.UserId.Value.ToString(),
                lifecycle.IdentityRevision,
                lifecycle.IdentityIncarnationId,
                state,
                lifecycle.Generation));
            if (!await WebSocketSendDeadline.SendAsync(
                    webSocket,
                    payload,
                    WebSocketMessageType.Text,
                    TimeSpan.FromSeconds(5),
                    ct,
                    throwOnCancellation: false))
            {
                throw new WebSocketException(
                    "Durable identity-withdrawal replay could not be delivered.");
            }
            if (registration.Queue.CompletionReason == CloseReason.AccessRevoked)
            {
                return;
            }
        }
    }

    private Task RevalidateAuthorizationPeriodicallyAsync(
        string token,
        UserId userId,
        string deviceId,
        string connectionId,
        Action<CloseReason> markAuthorizationFailure,
        CancellationTokenSource linkedCts,
        CancellationToken ct) =>
        WebSocketJwtRevalidationLoop.RunAsync(
            _jwtRevalidator,
            token,
            connectionId,
            "User-events",
            async () =>
            {
                markAuthorizationFailure(CloseReason.AuthRevoked);
                await linkedCts.CancelAsync();
            },
            _logger,
            _metrics,
            async revalidationCt =>
            {
                var authorized = (await _deviceAuthorization.EvaluateAsync(
                    userId,
                    deviceId,
                    revalidationCt)).Authorized;
                if (!authorized)
                {
                    markAuthorizationFailure(CloseReason.AccessRevoked);
                }
                return authorized;
            },
            ct);

    private static async Task ReceivePumpAsync(WebSocket webSocket, CancellationToken ct)
    {
        while (!ct.IsCancellationRequested && webSocket.State == WebSocketState.Open)
        {
            var readResult = await WebSocketMessageReader.ReceiveAsync(
                webSocket,
                InboundMaxMessageBytes,
                ct);

            if (readResult.Status == WebSocketMessageReader.ReadStatus.TooLarge)
            {
                webSocket.Abort();
                break;
            }

            if (readResult.Status != WebSocketMessageReader.ReadStatus.Message)
            {
                break;
            }

            if (readResult.MessageType is WebSocketMessageType.Text or WebSocketMessageType.Binary)
            {
                await WebSocketCloseHelper.CloseOutputAsync(
                    webSocket,
                    WebSocketCloseStatus.PolicyViolation,
                    CloseReason.ClientMessagesNotSupported.ToWire(),
                    ct);

                break;
            }
        }
    }

    internal static async Task SendPumpAsync(
        WebSocket webSocket,
        ChannelByteSendQueue sendQueue,
        CancellationToken ct,
        TimeSpan? sendTimeout = null)
    {
        var timeout = sendTimeout ?? TimeSpan.FromSeconds(5);
        while (!ct.IsCancellationRequested)
        {
            var message = await sendQueue.ReadAsync(ct);
            if (message is null)
            {
                break;
            }

            if (webSocket.State != WebSocketState.Open)
            {
                break;
            }

            if (!await WebSocketSendDeadline.SendAsync(
                    webSocket,
                    message,
                    WebSocketMessageType.Text,
                    timeout,
                    ct,
                    throwOnCancellation: false))
            {
                break;
            }
        }
    }
}
