using System.Net.WebSockets;
using System.Text;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Auth;
using Kodosi.Host.Observability;

namespace Kodosi.Host.Realtime;

internal sealed class ParticipantSessionPump(
    RelayMessageProcessor messageProcessor,
    IJwtRevalidator jwtRevalidator,
    OperationalMetrics metrics,
    ILogger<ParticipantSessionPump> logger)
{
    private static readonly TimeSpan DefaultSendDeadline = TimeSpan.FromSeconds(5);
    private readonly RelayMessageProcessor _messageProcessor = messageProcessor;
    private readonly IJwtRevalidator _jwtRevalidator = jwtRevalidator;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly ILogger<ParticipantSessionPump> _logger = logger;

    public async Task<ParticipantSessionPumpResult> RunAsync(
        WebSocket webSocket,
        ParticipantConnectionState state,
        string? accessToken,
        CancellationToken ct)
    {
        var sessionId = state.SessionId!.Value;
        var ports = state.Ports!;
        var accessDecision = state.AccessDecision!.Value;
        var linkedCts = state.LinkedCts!;
        var linkedToken = linkedCts.Token;
        var authRevoked = false;
        var participantQueue = state.ParticipantQueue!;
        var sendTask = SendPumpAsync(
            webSocket,
            participantQueue,
            linkedToken,
            participantQueue.CompletionToken);
        var receiveTask = ReceivePumpAsync(
            webSocket,
            ports.Host,
            ports.Participants,
            sessionId,
            state.ConnectionId,
            state.AuthenticatedUserId,
            accessDecision.AccessLevel,
            accessDecision.IsOwnerParticipant,
            state.DeviceId!,
            accessDecision.SessionIncarnationId,
            accessDecision.SessionIncarnationGeneration,
            state.ActionAuthority!,
            participantQueue,
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

        return new ParticipantSessionPumpResult(authRevoked);
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
            "Participant",
            async () =>
            {
                markAuthRevoked();
                await linkedCts.CancelAsync();
            },
            _logger,
            _metrics,
            ct);

    private async Task ReceivePumpAsync(
        WebSocket webSocket,
        ILiveSessionHostState host,
        ILiveParticipantRoster participants,
        SessionId sessionId,
        string connectionId,
        UserId authenticatedUserId,
        AccessLevel accessLevel,
        bool isOwnerParticipant,
        string deviceId,
        Guid sessionIncarnationId,
        long sessionIncarnationGeneration,
        IParticipantActionAuthority actionAuthority,
        RelayClientSendQueue participantQueue,
        CancellationToken ct)
    {
        var context = new RelayMessageContext(
            sessionId,
            connectionId,
            authenticatedUserId,
            accessLevel,
            isOwnerParticipant,
            host,
            participants,
            actionAuthority,
            deviceId,
            sessionIncarnationId,
            sessionIncarnationGeneration,
            participantQueue);

        while (!ct.IsCancellationRequested && webSocket.State == WebSocketState.Open)
        {
            var readResult = await WebSocketMessageReader.ReceiveAsync(
                webSocket,
                RelayMessageLimits.OuterMessageMaxBytes,
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

            if (readResult.MessageType != WebSocketMessageType.Text)
            {
                await WebSocketCloseHelper.CloseOutputAsync(
                    webSocket,
                    WebSocketCloseStatus.InvalidMessageType,
                    CloseReason.UnsupportedData.ToWire(),
                    ct);
                break;
            }

            var json = Encoding.UTF8.GetString(readResult.Payload);
            var outcome = await _messageProcessor.ProcessAsync(json, context, ct);
            if (outcome == RelayMessageProcessOutcome.ProtocolViolation)
            {
                await WebSocketCloseHelper.CloseOutputAsync(
                    webSocket,
                    WebSocketCloseStatus.PolicyViolation,
                    CloseReason.InvalidMessage.ToWire(),
                    ct);
                break;
            }
        }
    }

    internal static async Task SendPumpAsync(
        WebSocket webSocket,
        RelayClientSendQueue sendQueue,
        CancellationToken ct,
        CancellationToken queueCompletion,
        TimeSpan? sendDeadline = null)
    {
        var deadline = sendDeadline ?? DefaultSendDeadline;
        while (!ct.IsCancellationRequested)
        {
            var delivery = await sendQueue.ReadForSendAsync(ct);
            if (delivery is null || webSocket.State != WebSocketState.Open)
            {
                break;
            }

            var message = delivery.Value.Message;
            var messageType = message.Kind == WireMessageKind.EncryptedBinary
                ? WebSocketMessageType.Binary
                : WebSocketMessageType.Text;
            if (delivery.Value.IsTerminal)
            {
                _ = await WebSocketSendDeadline.SendAsync(
                    webSocket,
                    message.Payload,
                    messageType,
                    deadline,
                    ct,
                    throwOnCancellation: false);
                return;
            }

            if (queueCompletion.IsCancellationRequested)
            {
                if (sendQueue.TryTakePendingTerminalMessage(out var terminal))
                {
                    var terminalType =
                        terminal.Kind == WireMessageKind.EncryptedBinary
                            ? WebSocketMessageType.Binary
                            : WebSocketMessageType.Text;
                    _ = await WebSocketSendDeadline.SendAsync(
                        webSocket,
                        terminal.Payload,
                        terminalType,
                        deadline,
                        ct,
                        throwOnCancellation: false);
                }
                return;
            }

            if (!await WebSocketSendDeadline.SendAsync(
                    webSocket,
                    message.Payload,
                    messageType,
                    deadline,
                    ct))
            {
                return;
            }

            if (queueCompletion.IsCancellationRequested)
            {
                if (sendQueue.TryTakePendingTerminalMessage(out var terminal))
                {
                    var terminalType =
                        terminal.Kind == WireMessageKind.EncryptedBinary
                            ? WebSocketMessageType.Binary
                            : WebSocketMessageType.Text;
                    _ = await WebSocketSendDeadline.SendAsync(
                        webSocket,
                        terminal.Payload,
                        terminalType,
                        deadline,
                        ct,
                        throwOnCancellation: false);
                }
                return;
            }
        }
    }
}

internal readonly record struct ParticipantSessionPumpResult(bool AuthRevoked);
