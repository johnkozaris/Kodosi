using System.Net.WebSockets;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Auth;
using Kodosi.Host.Observability;

namespace Kodosi.Host.Realtime;

internal sealed class HostSessionPump(
    SessionBroadcaster broadcaster,
    HostMessageProcessor messageProcessor,
    IJwtRevalidator jwtRevalidator,
    OperationalMetrics metrics,
    ILogger<HostSessionPump> logger)
{
    private static readonly TimeSpan DefaultSendDeadline = TimeSpan.FromSeconds(5);
    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly HostMessageProcessor _messageProcessor = messageProcessor;
    private readonly IJwtRevalidator _jwtRevalidator = jwtRevalidator;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly ILogger<HostSessionPump> _logger = logger;

    public async Task<HostSessionPumpResult> RunAsync(
        WebSocket webSocket,
        HostConnectionState state,
        string? accessToken,
        CancellationToken ct)
    {
        var sessionId = state.SessionId!.Value;
        var ports = state.Ports!;
        var messageSource = new HostMessageSource(
            sessionId,
            state.ConnectionId,
            ports,
            state.SessionQueues!,
            ports.IncarnationId,
            state.ActivatedSessionIncarnationId!.Value,
            state.AuthenticatedUserId,
            state.DeviceId!,
            state.ActivatedSessionIncarnationGeneration!.Value);
        var sendQueue = state.HostQueue!;
        var linkedCts = state.LinkedCts!;
        var linkedToken = linkedCts.Token;
        var authRevoked = false;
        using var queueCts = CancellationTokenSource.CreateLinkedTokenSource(
            linkedToken,
            sendQueue.CompletionToken);

        var sendTask = SendPumpAsync(
            webSocket,
            sendQueue,
            sessionId,
            queueCts.Token);
        var receiveTask = HostReceivePumpAsync(
            webSocket,
            messageSource,
            linkedToken);
        var revalidateTask = accessToken is null
            ? Task.CompletedTask
            : RevalidateTokenPeriodicallyAsync(
                accessToken,
                state.ConnectionId,
                () => { authRevoked = true; },
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
        catch (Exception ex) when (receiveTask.IsCompletedSuccessfully && receiveTask.Result.ShouldClose)
        {


            _logger.LogDebug(
                ex,
                "Suppressed exception during close-time pump teardown for session {SessionId}",
                sessionId);
        }

        var receiveOutcome = receiveTask.IsCompletedSuccessfully
            ? receiveTask.Result
            : HostProcessOutcome.Continue;
        return new HostSessionPumpResult(
            authRevoked,
            receiveOutcome.CloseReason,
            receiveOutcome.HostEnded);
    }

    private Task RevalidateTokenPeriodicallyAsync(
        string token,
        string connectionId,
        Action markAuthRevoked,
        CancellationTokenSource linkedCts,
        CancellationToken ct) =>
        WebSocketJwtRevalidationLoop.RunAsync(
            _jwtRevalidator,
            token,
            connectionId,
            "Host",
            async () =>
            {
                markAuthRevoked();
                await linkedCts.CancelAsync();
            },
            _logger,
            _metrics,
            ct);

    private async Task<HostProcessOutcome> HostReceivePumpAsync(
        WebSocket webSocket,
        HostMessageSource source,
        CancellationToken ct)
    {
        var sessionId = source.SessionId;
        while (!ct.IsCancellationRequested && webSocket.State == WebSocketState.Open)
        {
            var readResult = await WebSocketMessageReader.ReceiveAsync(
                webSocket,
                RelayMessageLimits.HostReceiveMaxBytes,
                ct);

            if (readResult.Status == WebSocketMessageReader.ReadStatus.Closed)
            {
                break;
            }

            if (readResult.Status == WebSocketMessageReader.ReadStatus.TooLarge)
            {
                webSocket.Abort();
                break;
            }

            if (readResult.MessageType == WebSocketMessageType.Binary)
            {
                var binaryOutcome = await _messageProcessor.ProcessBinaryAsync(
                    readResult.Payload,
                    source,
                    ct);
                if (binaryOutcome.ShouldClose)
                {
                    var closeReason = binaryOutcome.CloseReason ?? CloseReason.UnsupportedData;
                    await WebSocketCloseHelper.CloseOutputAsync(
                        webSocket,
                        closeReason.ToWebSocketCloseStatus(),
                        closeReason.ToWire(),
                        ct);
                    return binaryOutcome;
                }

                _broadcaster.FlushPendingHostFences(sessionId);
                continue;
            }

            if (readResult.MessageType != WebSocketMessageType.Text)
            {
                continue;
            }

            var textOutcome = await _messageProcessor.ProcessAsync(
                readResult.Payload,
                source,
                ct);
            if (textOutcome.ShouldClose)
            {
                var closeReason = textOutcome.CloseReason ?? CloseReason.InvalidMessage;
                await WebSocketCloseHelper.CloseOutputAsync(
                    webSocket,
                    closeReason.ToWebSocketCloseStatus(),
                    closeReason.ToWire(),
                    ct);
                return textOutcome;
            }

            _broadcaster.FlushPendingHostFences(sessionId);
        }

        return HostProcessOutcome.Continue;
    }

    internal async Task SendPumpAsync(
        WebSocket webSocket,
        ChannelByteSendQueue sendQueue,
        SessionId sessionId,
        CancellationToken ct,
        TimeSpan? sendDeadline = null)
    {
        var deadline = sendDeadline ?? DefaultSendDeadline;
        while (!ct.IsCancellationRequested)
        {
            var message = await sendQueue.ReadAsync(ct);
            if (message is null || webSocket.State != WebSocketState.Open)
            {
                break;
            }

            if (!await WebSocketSendDeadline.SendAsync(
                    webSocket,
                    message,
                    WebSocketMessageType.Text,
                    deadline,
                    ct))
            {
                break;
            }
            _broadcaster.ContinueUnacknowledgedHostFenceReplay(sessionId);
            _broadcaster.FlushPendingHostFences(sessionId);
        }
    }
}

internal readonly record struct HostSessionPumpResult(
    bool AuthRevoked,
    CloseReason? CloseReason,
    bool HostEnded)
{
}
