using System.Collections.Frozen;
using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed partial class HostMessageProcessor(
    SessionBroadcaster broadcaster,
    SessionEndCoordinator sessionEndCoordinator,
    SessionLifecycleGate lifecycleGate,
    ILiveSessionStateDirectory runtimes,
    OperationalMetrics metrics,
    ILogger<HostMessageProcessor> logger,
    ISemanticRelayRepository semanticRelay,
    IConnectionRegistry connections,
    IActionDedupeCache dedupeCache,
    IPermissionDecisionAuditStore permissionDecisionAuditStore,
    IServiceScopeFactory scopeFactory)
{
    private const byte TerminalCheckpointFrameTypeByte = 0x03;
    private const byte TerminalRawBatchFrameTypeByte = 0x04;
    private const byte TerminalPresentationFrameTypeByte = 0x05;
    private const byte PendingPermissionsFrameTypeByte = 0x06;
    private static readonly FrozenDictionary<string, HostDispatchHandler> Dispatch =
        new Dictionary<string, HostDispatchHandler>(StringComparer.Ordinal)
        {
            ["key.rotation"] = static (
                processor,
                rawBytes,
                identity,
                _,
                stream,
                _) =>
            {
                return ValueTask.FromResult(
                    processor.HandleKeyRotation(rawBytes, identity, stream));
            },
            ["host.heartbeat"] = static async (
                processor,
                rawBytes,
                identity,
                host,
                _,
                ct) =>
            {
                if (!processor.HasMatchingSessionId(
                        rawBytes,
                        identity.SessionId))
                {
                    return HostProcessOutcome.ProtocolViolation(
                        CloseReason.InvalidMessage);
                }

                if (!processor.IsCurrent(identity))
                {
                    return StaleHost();
                }
                host.RecordHostHeartbeat();
                return HostProcessOutcome.Continue;
            },
            ["host.end"] = static async (
                processor,
                rawBytes,
                identity,
                _,
                _,
                _) =>
            {
                return await processor.HandleEndAsync(
                    rawBytes,
                    identity);
            },
            ["host.actionResult"] = static async (
                processor,
                rawBytes,
                identity,
                _,
                _,
                ct) =>
            {
                return await processor.HandleHostActionResultAsync(
                    rawBytes,
                    identity,
                    ct);
            },
            ["host.semanticReceipt"] = static async (
                processor,
                rawBytes,
                identity,
                _,
                _,
                ct) =>
            {
                return await processor.HandleSemanticReceiptAsync(
                    rawBytes,
                    identity,
                    ct);
            },
            ["host.fenceAck"] = static (
                processor,
                rawBytes,
                identity,
                _,
                _,
                _) =>
            {
                return ValueTask.FromResult(
                    processor.HandleFenceAck(
                        rawBytes,
                        identity.SessionId));
            },
        }.ToFrozenDictionary(StringComparer.Ordinal);

    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly SessionEndCoordinator _sessionEndCoordinator = sessionEndCoordinator;
    private readonly SessionLifecycleGate _lifecycleGate = lifecycleGate;
    private readonly ILiveSessionStateDirectory _runtimes = runtimes;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly ILogger<HostMessageProcessor> _logger = logger;
    private readonly ISemanticRelayRepository _semanticRelay = semanticRelay;
    private readonly IConnectionRegistry _connections = connections;
    private readonly IPermissionDecisionAuditStore _permissionDecisionAuditStore =
        permissionDecisionAuditStore;
    private readonly IActionDedupeCache _dedupeCache = dedupeCache;
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;

    public async Task<HostProcessOutcome> ProcessAsync(
        byte[] rawBytes,
        HostMessageSource source,
        CancellationToken ct)
    {
        try
        {
            var envelope = JsonSerializer.Deserialize(rawBytes, WsJsonContext.Default.WsEnvelope);
            if (envelope is null)
            {
                return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
            }

            if (string.IsNullOrWhiteSpace(envelope.Type))
            {
                return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
            }

            if (Dispatch.TryGetValue(envelope.Type, out var handler))
            {
                if (!RelayMessageLimits.IsWithinLimit(envelope.Type, rawBytes.Length))
                {
                    return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
                }

                if (string.Equals(
                        envelope.Type,
                        "host.end",
                        StringComparison.Ordinal))
                {
                    return await handler(
                        this,
                        rawBytes,
                        source,
                        source.Runtime.Host,
                        source.Runtime.Stream,
                        ct);
                }

                await using var lifecycle =
                    await _lifecycleGate.AcquireAsync(source.SessionId, ct);
                if (!IsCurrent(source))
                {
                    return StaleHost();
                }
                return await handler(
                    this,
                    rawBytes,
                    source,
                    source.Runtime.Host,
                    source.Runtime.Stream,
                    ct);
            }

            _logger.LogWarning("Unsupported host message type {MessageType}", envelope.Type);
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }
        catch (JsonException ex)
        {
            _logger.LogWarning(ex, "Failed to parse host message");
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }
    }

    internal bool IsCurrent(HostMessageSource source) =>
        source.Runtime.SessionId == source.SessionId
        && source.Runtime.IncarnationId == source.RuntimeIncarnationId
        && ReferenceEquals(_runtimes.TryGet(source.SessionId), source.Runtime)
        && ReferenceEquals(
            _broadcaster.TryGetSession(source.SessionId),
            source.Queues)
        && string.Equals(
            source.Runtime.Host.HostConnectionId ?? string.Empty,
            source.ConnectionId,
            StringComparison.Ordinal);

    private static HostProcessOutcome StaleHost() =>
        HostProcessOutcome.ProtocolViolation(CloseReason.SessionNotLive);

    private bool HasMatchingSessionId(byte[] rawBytes, SessionId sessionId)
    {
        using var document = JsonDocument.Parse(rawBytes);
        return HasMatchingSessionId(document.RootElement, sessionId);
    }

    private static bool HasMatchingSessionId(JsonElement root, SessionId sessionId)
        => TryGetRequiredString(root, "sessionId", out var value)
            && Guid.TryParse(value, out var parsed)
            && parsed == sessionId.Value;

    private static bool TryGetRequiredString(JsonElement root, string propertyName, out string value)
    {
        value = string.Empty;
        if (!root.TryGetProperty(propertyName, out var property)
            || property.ValueKind != JsonValueKind.String)
        {
            return false;
        }

        var parsed = property.GetString();
        if (parsed is null)
        {
            return false;
        }

        value = parsed;
        return true;
    }

    private static bool TryGetRequiredInt32(JsonElement root, string propertyName, out int value)
    {
        value = default;
        return root.TryGetProperty(propertyName, out var property)
            && property.ValueKind == JsonValueKind.Number
            && property.TryGetInt32(out value);
    }

    private HostProcessOutcome HandleFenceAck(byte[] rawBytes, SessionId sessionId)
    {
        var ack = JsonSerializer.Deserialize(
            rawBytes,
            WsJsonContext.Default.HostFenceAckMessage);
        if (ack is null
            || !Guid.TryParse(ack.SessionId, out var parsedSessionId)
            || parsedSessionId != sessionId.Value
            || !Guid.TryParseExact(ack.FenceId, "N", out _))
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }
        _broadcaster.AcknowledgeHostFence(sessionId, ack.FenceId);
        return HostProcessOutcome.Continue;
    }

    private delegate ValueTask<HostProcessOutcome> HostDispatchHandler(
        HostMessageProcessor processor,
        byte[] rawBytes,
        HostMessageSource identity,
        ILiveSessionHostState host,
        ILiveSessionStreamCache stream,
        CancellationToken ct);
}

internal readonly record struct HostMessageSource(
    SessionId SessionId,
    string ConnectionId,
    LiveSessionPorts Runtime,
    SessionSendQueues Queues,
    Guid RuntimeIncarnationId,
    Guid SessionIncarnationId = default,
    UserId AuthenticatedUserId = default,
    string DeviceId = "",
    long SessionIncarnationGeneration = default);

internal enum HostProcessOutcomeKind
{
    Continue,
    End,
    ProtocolViolation,
}

internal readonly record struct HostProcessOutcome(HostProcessOutcomeKind Kind, CloseReason? CloseReason)
{
    public static HostProcessOutcome Continue { get; } = new(HostProcessOutcomeKind.Continue, null);

    public bool ShouldClose => Kind != HostProcessOutcomeKind.Continue;

    public bool HostEnded => Kind == HostProcessOutcomeKind.End;

    public static HostProcessOutcome End(CloseReason closeReason)
        => new(HostProcessOutcomeKind.End, closeReason);

    public static HostProcessOutcome ProtocolViolation(CloseReason closeReason)
        => new(HostProcessOutcomeKind.ProtocolViolation, closeReason);
}
