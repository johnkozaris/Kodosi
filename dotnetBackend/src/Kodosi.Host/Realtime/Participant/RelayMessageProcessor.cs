using System.Collections.Frozen;
using System.Text;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal readonly record struct RelayMessageContext(
    SessionId SessionId,
    string ConnectionId,
    UserId UserId,
    AccessLevel AccessLevel,


    bool IsOwnerParticipant,
    ILiveSessionHostState Host,
    ILiveParticipantRoster Participants,
    IParticipantActionAuthority ActionAuthority,
    string DeviceId = "",
    Guid SessionIncarnationId = default,
    long SessionIncarnationGeneration = default,
    RelayClientSendQueue? ParticipantQueue = null)
{
    public SessionCapabilities Capabilities =>
        SessionCapabilities.FromAccess(AccessLevel, IsOwnerParticipant);
}

internal enum RelayMessageProcessOutcome
{
    Handled,
    ProtocolViolation,
}

internal sealed partial class RelayMessageProcessor(
    SessionBroadcaster broadcaster,
    IActionDedupeCache dedupeCache,
    IAuditWriter auditWriter,
    ISemanticRelayRepository semanticRelay,
    OperationalMetrics metrics,
    ILogger<RelayMessageProcessor> logger,
    IServiceScopeFactory scopeFactory,
    IPermissionDecisionAuditStore permissionDecisionAuditStore)
{
    private static readonly FrozenDictionary<string, ParticipantDispatchHandler> Dispatch =
        new Dictionary<string, ParticipantDispatchHandler>(StringComparer.Ordinal)
        {
            ["participant.semanticSend"] = static async (processor, json, context, ct) =>
            {
                var message = JsonSerializer.Deserialize(
                    json,
                    WsJsonContext.Default.ParticipantSemanticSendMessage);
                if (message is null || !IsValidSemanticSend(message))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                await processor.HandleSemanticSendAsync(message, context, ct);
                return RelayMessageProcessOutcome.Handled;
            },
            ["participant.semanticCancel"] = static async (processor, json, context, ct) =>
            {
                var message = JsonSerializer.Deserialize(
                    json,
                    WsJsonContext.Default.ParticipantSemanticCancelMessage);
                if (message is null || !IsValidSemanticCancel(message))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                await processor.HandleSemanticCancelAsync(message, context, ct);
                return RelayMessageProcessOutcome.Handled;
            },
            ["participant.semanticReceiptAck"] = static async (processor, json, context, ct) =>
            {
                var message = JsonSerializer.Deserialize(
                    json,
                    WsJsonContext.Default.ParticipantSemanticReceiptAckMessage);
                if (message is null || !IsValidSemanticReceiptAck(message))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                return await processor.HandleSemanticReceiptAckAsync(
                    message,
                    context,
                    ct);
            },
            ["participant.heartbeat"] = static (_, _, _, _) =>
                ValueTask.FromResult(RelayMessageProcessOutcome.Handled),
            ["participant.suggest"] = static async (processor, json, context, ct) =>
            {
                var suggest = JsonSerializer.Deserialize(json, WsJsonContext.Default.ParticipantSuggestMessage);
                if (suggest is null || !IsValidParticipantSuggest(suggest))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                await processor.HandleSuggestAsync(suggest, context, ct);
                return RelayMessageProcessOutcome.Handled;
            },
            ["participant.inject"] = static async (processor, json, context, ct) =>
            {
                var inject = JsonSerializer.Deserialize(json, WsJsonContext.Default.ParticipantInjectMessage);
                if (inject is null || !IsValidParticipantInject(inject))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                await processor.HandleInjectAsync(inject, context, ct);
                return RelayMessageProcessOutcome.Handled;
            },
            ["participant.resize"] = static async (processor, json, context, ct) =>
            {
                var resize = JsonSerializer.Deserialize(json, WsJsonContext.Default.ParticipantResizeMessage);
                if (resize is null || !IsValidParticipantResize(resize))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                await processor.HandleResizeAsync(resize, context, ct);
                return RelayMessageProcessOutcome.Handled;
            },
            ["participant.focusChanged"] = static async (processor, json, context, ct) =>
            {
                var focusChanged = JsonSerializer.Deserialize(json, WsJsonContext.Default.ParticipantFocusChangedMessage);
                if (focusChanged is null || !IsValidParticipantFocusChanged(focusChanged))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                await processor.HandleFocusChangedAsync(focusChanged, context, ct);
                return RelayMessageProcessOutcome.Handled;
            },
            ["participant.permissionDecision"] = static async (processor, json, context, ct) =>
            {
                var decision = JsonSerializer.Deserialize(json, WsJsonContext.Default.ParticipantPermissionDecisionMessage);
                if (decision is null || !IsValidPermissionDecision(decision))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                await processor.HandlePermissionDecisionAsync(decision, context, ct);
                return RelayMessageProcessOutcome.Handled;
            },
            ["participant.stop"] = static async (processor, json, context, ct) =>
            {
                var stop = JsonSerializer.Deserialize(json, WsJsonContext.Default.ParticipantStopMessage);
                if (stop is null || !IsValidOwnerLifecycle(stop.ActionId, stop.Nonce, stop.Ciphertext, stop.Signature))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                await processor.HandleStopAsync(stop, context, ct);
                return RelayMessageProcessOutcome.Handled;
            },
            ["participant.interrupt"] = static async (processor, json, context, ct) =>
            {
                var interrupt = JsonSerializer.Deserialize(json, WsJsonContext.Default.ParticipantInterruptMessage);
                if (interrupt is null || !IsValidOwnerLifecycle(
                        interrupt.ActionId,
                        interrupt.Nonce,
                        interrupt.Ciphertext,
                        interrupt.Signature))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                await processor.HandleInterruptAsync(interrupt, context, ct);
                return RelayMessageProcessOutcome.Handled;
            },
        }.ToFrozenDictionary(StringComparer.Ordinal);



    internal const ushort MaxResizeRows = 512;
    internal const ushort MaxResizeCols = 1024;

    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly IActionDedupeCache _dedupeCache = dedupeCache;
    private readonly IAuditWriter _auditWriter = auditWriter;
    private readonly ISemanticRelayRepository _semanticRelay = semanticRelay;
    private readonly IPermissionDecisionAuditStore _permissionDecisionAuditStore =
        permissionDecisionAuditStore;
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly ILogger<RelayMessageProcessor> _logger = logger;

    public async Task<RelayMessageProcessOutcome> ProcessAsync(
        string json,
        RelayMessageContext context,
        CancellationToken ct)
    {
        try
        {
            var envelope = JsonSerializer.Deserialize(json, WsJsonContext.Default.WsEnvelope);
            if (envelope is null)
            {
                return RelayMessageProcessOutcome.ProtocolViolation;
            }

            if (string.IsNullOrWhiteSpace(envelope.Type))
            {
                return RelayMessageProcessOutcome.ProtocolViolation;
            }

            if (Dispatch.TryGetValue(envelope.Type, out var handler))
            {
                if (!RelayMessageLimits.IsWithinLimit(
                        envelope.Type,
                        Encoding.UTF8.GetByteCount(json)))
                {
                    return RelayMessageProcessOutcome.ProtocolViolation;
                }

                var outcome = await handler(this, json, context, ct);
                if (outcome == RelayMessageProcessOutcome.Handled)
                {
                    context.Participants.RecordParticipantActivity(context.ConnectionId);
                }

                return outcome;
            }

            _logger.LogWarning("Unsupported participant message type {MessageType}", envelope.Type);
            return RelayMessageProcessOutcome.ProtocolViolation;
        }
        catch (JsonException ex)
        {
            _logger.LogWarning(ex, "Failed to parse participant message");
            return RelayMessageProcessOutcome.ProtocolViolation;
        }
        catch (RelayMessageLimitException ex)
        {
            _logger.LogWarning(
                "Participant message {MessageType} transformed into an oversized host message",
                ex.ParticipantMessageType);
            return RelayMessageProcessOutcome.ProtocolViolation;
        }
    }

    private delegate ValueTask<RelayMessageProcessOutcome> ParticipantDispatchHandler(
        RelayMessageProcessor processor,
        string json,
        RelayMessageContext context,
        CancellationToken ct);

    private static void EnsureTransformedOutputWithinLimit(
        string participantMessageType,
        byte[] output)
    {
        if (!RelayMessageLimits.IsTransformedParticipantOutputWithinLimit(
                participantMessageType,
                output.Length))
        {
            throw new RelayMessageLimitException(participantMessageType);
        }
    }

    private sealed class RelayMessageLimitException(string participantMessageType)
        : Exception
    {
        public string ParticipantMessageType { get; } = participantMessageType;
    }
}
