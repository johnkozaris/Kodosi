using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed partial class HostMessageProcessor
{
    private async Task<HostProcessOutcome> HandleHostActionResultAsync(
        byte[] rawBytes,
        HostMessageSource source,
        CancellationToken ct)
    {
        var result = JsonSerializer.Deserialize(
            rawBytes,
            WsJsonContext.Default.HostActionResultMessage);
        if (result is null
            || !HasMatchingSessionId(rawBytes, source.SessionId)
            || result.IncarnationId != source.SessionIncarnationId
            || result.Status is not (RelayActionStatus.Accepted or RelayActionStatus.Rejected)
            || !Guid.TryParse(result.RequesterUserId, out var requesterGuid)
            || requesterGuid == Guid.Empty
            || string.IsNullOrWhiteSpace(result.ActionId)
            || string.IsNullOrWhiteSpace(result.RequestId)
            || result.RequestGeneration <= 0
            || string.IsNullOrWhiteSpace(result.RequesterDeviceId))
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }
        var assertedTuple = new PermissionDecisionPendingTuple(
            result.IncarnationId,
            source.SessionIncarnationGeneration,
            result.RequestId,
            result.RequestGeneration,
            result.RequesterDeviceId);
        HostActionCompletionResult completion;
        try
        {
            completion = await _permissionDecisionAuditStore.CompleteHostActionAsync(
                source.SessionId,
                UserId.From(requesterGuid),
                result.ActionId,
                assertedTuple,
                result.Status == RelayActionStatus.Accepted
                    ? InputAuditStatus.Dispatched
                    : InputAuditStatus.Rejected,
                ct);
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            _logger.LogError(
                ex,
                "Failed to durably complete host action result {ActionId}",
                result.ActionId);
            return HostProcessOutcome.ProtocolViolation(CloseReason.ServerError);
        }
        if (completion.Outcome is HostActionCompletionOutcome.NotFound
            or HostActionCompletionOutcome.Conflict
            || completion.CanonicalTuple is not { } canonicalTuple)
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }
        if (!IsCurrent(source))
        {
            return StaleHost();
        }
        var requesterUserId = completion.RequesterUserId;
        var actionId = completion.ActionId;
        var bytes = RelayOutbound.Encode(new ActionResultMessage(
            source.SessionId.Value.ToString(),
            actionId,
            result.Status,
            canonicalTuple.RequestId,
            canonicalTuple.RequestGeneration));
        foreach (var connection in _connections.GetConnectionsForDevice(
                     requesterUserId,
                     canonicalTuple.RequesterDeviceId))
        {
            if (connection.SessionId == source.SessionId
                && connection.Kind is SessionConnectionKind.OwnerParticipant
                    or SessionConnectionKind.SharedParticipant
                && _broadcaster.TryGetParticipantQueue(
                    source.SessionId,
                    connection.ConnectionId) is { } expectedQueue)
            {
                _ = _broadcaster.SendToParticipantIfSame(
                    source.SessionId,
                    connection.ConnectionId,
                    expectedQueue,
                    WireMessage.Json(bytes));
            }
        }
        _dedupeCache.CompleteCurrent(
            source.SessionId,
            requesterUserId,
            actionId,
            result.Status == RelayActionStatus.Accepted
                ? ActionDedupeFinalOutcome.Accepted
                : ActionDedupeFinalOutcome.Rejected);
        if (!_broadcaster.SendHostActionResultAckIfSame(
                source.SessionId,
                source.Queues,
                canonicalTuple.SessionIncarnationId,
                actionId,
                requesterUserId.Value.ToString()))
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.ServerError);
        }
        return HostProcessOutcome.Continue;
    }

    private async Task<HostProcessOutcome> HandleSemanticReceiptAsync(
        byte[] rawBytes,
        HostMessageSource source,
        CancellationToken ct)
    {
        var receipt = JsonSerializer.Deserialize(
            rawBytes,
            WsJsonContext.Default.HostSemanticReceiptMessage);
        if (receipt is null
            || !HasMatchingSessionId(rawBytes, source.SessionId)
            || receipt.IncarnationId != source.SessionIncarnationId
            || !SemanticRelayWireRules.IsCanonicalUuidV7(receipt.RequestId)
            || !SemanticRelayWireRules.IsMode(SemanticModeWire(receipt.Mode))
            || !SemanticRelayWireRules.IsLowerHexSha256(receipt.PayloadSha256)
            || !SemanticRelayWireRules.IsOutcome(SemanticOutcomeWire(receipt.Outcome))
            || !Guid.TryParse(receipt.RequesterUserId, out var requesterUserGuid)
            || requesterUserGuid == Guid.Empty
            || !Guid.TryParse(receipt.OwnerUserId, out var ownerUserGuid)
            || UserId.From(ownerUserGuid) != source.AuthenticatedUserId
            || !string.Equals(receipt.OwnerDeviceId, source.DeviceId, StringComparison.Ordinal)
            || string.IsNullOrWhiteSpace(receipt.RequesterDeviceId)
            || string.IsNullOrWhiteSpace(receipt.Signature))
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }

        var requesterUserId = UserId.From(requesterUserGuid);
        if (requesterUserId != source.AuthenticatedUserId)
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }

        var envelope = new SemanticReceiptEnvelope(
            source.SessionId,
            receipt.IncarnationId,
            receipt.RequestId,
            SemanticModeWire(receipt.Mode),
            receipt.PayloadSha256,
            SemanticOutcomeWire(receipt.Outcome),
            requesterUserId,
            receipt.RequesterDeviceId,
            source.AuthenticatedUserId,
            source.DeviceId,
            receipt.Signature);
        await using (var scope = _scopeFactory.CreateAsyncScope())
        {
            var verifier = scope.ServiceProvider.GetRequiredService<ISemanticReceiptVerifier>();
            if (!await verifier.VerifyAsync(
                    source.AuthenticatedUserId,
                    source.DeviceId,
                    envelope,
                    ct))
            {
                return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
            }
        }

        var stored = await _semanticRelay.StoreReceiptAsync(
            source.SessionId,
            receipt.IncarnationId,
            requesterUserId,
            receipt.RequesterDeviceId,
            receipt.RequestId,
            SemanticModeWire(receipt.Mode),
            receipt.PayloadSha256,
            SemanticOutcomeWire(receipt.Outcome),
            source.AuthenticatedUserId,
            source.DeviceId,
            receipt.Signature,
            ct);
        if (!stored)
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }
        if (!IsCurrent(source))
        {
            return StaleHost();
        }

        var participantMessage = new ParticipantSemanticReceiptMessage(
            source.SessionId.Value.ToString(),
            receipt.IncarnationId,
            receipt.RequestId,
            receipt.Mode,
            receipt.PayloadSha256,
            receipt.Outcome,
            receipt.RequesterUserId,
            receipt.RequesterDeviceId,
            receipt.OwnerUserId,
            receipt.OwnerDeviceId,
            receipt.Signature);
        var participantDelivery = SemanticReceiptWire.Delivery(participantMessage);
        if (!RelayMessageLimits.IsWithinLimit(
                "participant.semanticReceipt",
                participantDelivery.Message.Payload.Length))
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.InvalidMessage);
        }

        foreach (var participant in source.Queues.GetSemanticReceiptParticipants(
                     requesterUserId,
                     receipt.RequesterDeviceId))
        {
            _broadcaster.SendSemanticReceiptToParticipantIfSame(
                source.SessionId,
                participant.ConnectionId,
                participant.Queue,
                participantDelivery.RequestId,
                participantDelivery.Message);
        }

        if (!_broadcaster.SendHostSemanticReceiptAckIfSame(
                source.SessionId,
                source.Queues,
                receipt.IncarnationId,
                receipt.RequestId,
                receipt.RequesterUserId,
                receipt.RequesterDeviceId))
        {
            return HostProcessOutcome.ProtocolViolation(CloseReason.ServerError);
        }
        return HostProcessOutcome.Continue;
    }

    private static string SemanticModeWire(RelaySemanticMode mode) => mode switch
    {
        RelaySemanticMode.Queue => SemanticRelayWireRules.QueueMode,
        RelaySemanticMode.Steer => SemanticRelayWireRules.SteerMode,
        RelaySemanticMode.StopAndSend => SemanticRelayWireRules.StopAndSendMode,
        _ => throw new ArgumentOutOfRangeException(nameof(mode)),
    };

    private static string SemanticOutcomeWire(RelaySemanticOutcome outcome) => outcome switch
    {
        RelaySemanticOutcome.Injected => SemanticRelayWireRules.InjectedOutcome,
        RelaySemanticOutcome.Cancelled => SemanticRelayWireRules.CancelledOutcome,
        RelaySemanticOutcome.DeliveryUnknown => SemanticRelayWireRules.DeliveryUnknownOutcome,
        _ => throw new ArgumentOutOfRangeException(nameof(outcome)),
    };
}
