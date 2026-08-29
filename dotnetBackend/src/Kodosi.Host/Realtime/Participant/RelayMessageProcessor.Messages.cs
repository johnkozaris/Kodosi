using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed partial class RelayMessageProcessor
{
    private const int MaxActionIdLength = 128;

    private static string SemanticModeWire(RelaySemanticMode mode) => mode switch
    {
        RelaySemanticMode.Queue => SemanticRelayWireRules.QueueMode,
        RelaySemanticMode.Steer => SemanticRelayWireRules.SteerMode,
        RelaySemanticMode.StopAndSend => SemanticRelayWireRules.StopAndSendMode,
        _ => throw new ArgumentOutOfRangeException(nameof(mode)),
    };

    private static bool IsValidSemanticSend(ParticipantSemanticSendMessage message) =>
        SemanticRelayWireRules.IsCanonicalUuidV7(message.RequestId)
        && SemanticRelayWireRules.IsCanonicalUuidV7(message.IncarnationId)
        && SemanticRelayWireRules.IsMode(SemanticModeWire(message.Mode))
        && SemanticRelayWireRules.IsLowerHexSha256(message.PayloadSha256)
        && !string.IsNullOrWhiteSpace(message.Nonce)
        && !string.IsNullOrWhiteSpace(message.Ciphertext)
        && !string.IsNullOrWhiteSpace(message.Signature);

    private static bool IsValidSemanticCancel(ParticipantSemanticCancelMessage message) =>
        SemanticRelayWireRules.IsCanonicalUuidV7(message.RequestId)
        && SemanticRelayWireRules.IsCanonicalUuidV7(message.IncarnationId)
        && SemanticRelayWireRules.IsMode(SemanticModeWire(message.Mode))
        && SemanticRelayWireRules.IsLowerHexSha256(message.PayloadSha256)
        && !string.IsNullOrWhiteSpace(message.Signature);
    private static bool IsValidSemanticReceiptAck(
        ParticipantSemanticReceiptAckMessage message) =>
        Guid.TryParse(message.SessionId, out var sessionId)
        && sessionId != Guid.Empty
        && SemanticRelayWireRules.IsCanonicalUuidV7(message.IncarnationId)
        && SemanticRelayWireRules.IsCanonicalUuidV7(message.RequestId)
        && Guid.TryParse(message.RequesterUserId, out var requesterUserId)
        && requesterUserId != Guid.Empty
        && !string.IsNullOrWhiteSpace(message.RequesterDeviceId)
        && !string.IsNullOrWhiteSpace(message.Signature);

    private static bool IsValidActionId(string actionId) =>
        !string.IsNullOrWhiteSpace(actionId)
        && actionId.Length <= MaxActionIdLength;

    private static bool IsValidParticipantSuggest(ParticipantSuggestMessage message)
        => IsValidActionId(message.ActionId)
            && message.Nonce is { Length: > 0 }
            && message.Ciphertext is { Length: > 0 }
            && message.Signature is { Length: > 0 };

    private static bool IsValidParticipantInject(ParticipantInjectMessage message)
        => IsValidActionId(message.ActionId)
            && message.Nonce is { Length: > 0 }
            && message.Ciphertext is { Length: > 0 }
            && message.Signature is { Length: > 0 };



    private static bool IsValidPermissionDecision(ParticipantPermissionDecisionMessage message)
    {
        if (!IsValidActionId(message.ActionId)
            || !IsValidActionId(message.RequestId)
            || message.RequestGeneration <= 0)
        {
            return false;
        }

        return message.Decision is "allow" or "deny"
            && message.Nonce is { Length: > 0 }
            && message.Ciphertext is { Length: > 0 }
            && message.Signature is { Length: > 0 };
    }

    private static bool IsValidParticipantResize(ParticipantResizeMessage message)
        => IsValidActionId(message.ActionId)
            && message.Rows is > 0 and <= MaxResizeRows
            && message.Cols is > 0 and <= MaxResizeCols
            && ((message.WidthPixels is null
                    && message.HeightPixels is null
                    && message.CellWidthPixels is null
                    && message.CellHeightPixels is null)
                || (message.WidthPixels is > 0
                    && message.HeightPixels is > 0
                    && message.CellWidthPixels is > 0
                    && message.CellHeightPixels is > 0))
            && message.Nonce is { Length: > 0 }
            && message.Ciphertext is { Length: > 0 }
            && message.Signature is { Length: > 0 };

    private static bool IsValidParticipantFocusChanged(ParticipantFocusChangedMessage message)
        => IsValidActionId(message.ActionId)
            && message.Focused.HasValue
            && message.Nonce is { Length: > 0 }
            && message.Ciphertext is { Length: > 0 }
            && message.Signature is { Length: > 0 };

    private static bool IsValidOwnerLifecycle(
        string actionId,
        string nonce,
        string ciphertext,
        string signature)
        => IsValidActionId(actionId)
            && !string.IsNullOrWhiteSpace(nonce)
            && !string.IsNullOrWhiteSpace(ciphertext)
            && !string.IsNullOrWhiteSpace(signature);

    private async Task HandleSemanticSendAsync(
        ParticipantSemanticSendMessage message,
        RelayMessageContext context,
        CancellationToken ct)
    {
        if (!context.IsOwnerParticipant
            || message.IncarnationId != context.SessionIncarnationId)
        {
            await RejectActionAsync(
                context,
                message.RequestId.ToString("D"),
                InputAuditKind.Inject,
                message.PayloadSha256,
                ct);
            return;
        }
        var claim = await _semanticRelay.ClaimRequestAsync(
            context.SessionId,
            message.IncarnationId,
            context.UserId,
            context.DeviceId,
            message.RequestId,
            SemanticModeWire(message.Mode),
            message.PayloadSha256,
            ct);
        if (claim.Kind == SemanticRequestClaimKind.Conflict)
        {
            await RejectActionAsync(
                context,
                message.RequestId.ToString("D"),
                InputAuditKind.Inject,
                message.PayloadSha256,
                ct);
            return;
        }
        var ownerBytes = RelayOutbound.Encode(new HostSemanticSendMessage(
            context.SessionId.Value.ToString(),
            message.RequestId,
            message.IncarnationId,
            message.Mode,
            message.PayloadSha256,
            message.Nonce,
            message.Ciphertext,
            context.UserId.Value.ToString(),
            context.DeviceId,
            message.Signature));
        EnsureTransformedOutputWithinLimit("participant.semanticSend", ownerBytes);
        var dispatch = await context.ActionAuthority.TryDispatchAsync(
            SessionCapability.Stop,
            () => _broadcaster.SendToHost(context.SessionId, ownerBytes),
            ct);
        if (dispatch.Authorized
            && dispatch.SendOutcome == ChannelByteSendQueueWriteOutcome.Enqueued)
        {
            await _semanticRelay.MarkDispatchedAsync(claim.Request.Id, ct);
        }
    }

    private async Task HandleSemanticCancelAsync(
        ParticipantSemanticCancelMessage message,
        RelayMessageContext context,
        CancellationToken ct)
    {
        if (!context.IsOwnerParticipant
            || message.IncarnationId != context.SessionIncarnationId)
        {
            await RejectActionAsync(
                context,
                message.RequestId.ToString("D"),
                InputAuditKind.Inject,
                message.PayloadSha256,
                ct);
            return;
        }
        var claim = await _semanticRelay.FindExactRequestAsync(
            context.SessionId,
            message.IncarnationId,
            context.UserId,
            context.DeviceId,
            message.RequestId,
            SemanticModeWire(message.Mode),
            message.PayloadSha256,
            ct);
        if (claim is null || claim.Kind == SemanticRequestClaimKind.Conflict)
        {
            await RejectActionAsync(
                context,
                message.RequestId.ToString("D"),
                InputAuditKind.Inject,
                message.PayloadSha256,
                ct);
            return;
        }
        var ownerBytes = RelayOutbound.Encode(new HostSemanticCancelMessage(
            context.SessionId.Value.ToString(),
            message.RequestId,
            message.IncarnationId,
            message.Mode,
            message.PayloadSha256,
            context.UserId.Value.ToString(),
            context.DeviceId,
            message.Signature));
        EnsureTransformedOutputWithinLimit("participant.semanticCancel", ownerBytes);
        await context.ActionAuthority.TryDispatchAsync(
            SessionCapability.Stop,
            () => _broadcaster.SendToHost(context.SessionId, ownerBytes),
            ct);
    }

    private async ValueTask<RelayMessageProcessOutcome> HandleSemanticReceiptAckAsync(
        ParticipantSemanticReceiptAckMessage message,
        RelayMessageContext context,
        CancellationToken ct)
    {
        if (!context.IsOwnerParticipant
            || !Guid.TryParse(message.SessionId, out var sessionGuid)
            || sessionGuid != context.SessionId.Value
            || message.IncarnationId != context.SessionIncarnationId
            || !Guid.TryParse(message.RequesterUserId, out var requesterUserGuid)
            || UserId.From(requesterUserGuid) != context.UserId
            || !string.Equals(
                message.RequesterDeviceId,
                context.DeviceId,
                StringComparison.Ordinal))
        {
            return RelayMessageProcessOutcome.ProtocolViolation;
        }

        var expectedQueue = _broadcaster.TryGetParticipantQueue(
            context.SessionId,
            context.ConnectionId);
        if (expectedQueue is null)
        {
            return RelayMessageProcessOutcome.ProtocolViolation;
        }

        await using var scope = _scopeFactory.CreateAsyncScope();
        var verifier = scope.ServiceProvider
            .GetRequiredService<SemanticReceiptAckVerifier>();
        if (!await verifier.VerifyAsync(
                context.SessionId,
                message.IncarnationId,
                message.RequestId,
                context.UserId,
                context.DeviceId,
                message.RequesterUserId,
                message.RequesterDeviceId,
                message.Signature,
                ct))
        {
            return RelayMessageProcessOutcome.ProtocolViolation;
        }

        if (!await _semanticRelay.AcknowledgeReceiptAsync(
                context.SessionId,
                message.IncarnationId,
                context.UserId,
                context.DeviceId,
                message.RequestId,
                ct))
        {
            return RelayMessageProcessOutcome.ProtocolViolation;
        }

        var next = await _semanticRelay.ListPendingReceiptsAsync(
            context.SessionId,
            message.IncarnationId,
            context.UserId,
            context.DeviceId,
            limit: 1,
            ct);
        if (next.Count == 0)
        {
            return RelayMessageProcessOutcome.Handled;
        }
        _ = _broadcaster.SendToParticipantIfSame(
            context.SessionId,
            context.ConnectionId,
            expectedQueue,
            WireMessage.Json(SemanticReceiptWire.Encode(next[0])));
        return RelayMessageProcessOutcome.Handled;
    }

    private async Task HandleSuggestAsync(
        ParticipantSuggestMessage suggest,
        RelayMessageContext context,
        CancellationToken ct)
    {
        var ownerMessage = new HostSuggestionMessage(
            context.SessionId.Value.ToString(),
            suggest.ActionId,
            suggest.Nonce,
            suggest.Ciphertext,
            context.UserId.Value.ToString(),
            context.DeviceId,
            suggest.Signature);
        var ownerBytes = RelayOutbound.Encode(ownerMessage);
        EnsureTransformedOutputWithinLimit("participant.suggest", ownerBytes);
        await HandleActionAsync(
            context,
            suggest.ActionId,
            suggest.Ciphertext,
            SessionCapability.Suggest,
            InputAuditKind.Suggestion,
            ownerBytes,
            ct);
    }

    private async Task HandleInjectAsync(
        ParticipantInjectMessage inject,
        RelayMessageContext context,
        CancellationToken ct)
    {
        var ownerMessage = new HostInputMessage(
            context.SessionId.Value.ToString(),
            inject.ActionId,
            inject.Nonce,
            inject.Ciphertext,
            context.UserId.Value.ToString(),
            context.DeviceId,
            inject.Signature);
        var ownerBytes = RelayOutbound.Encode(ownerMessage);
        EnsureTransformedOutputWithinLimit("participant.inject", ownerBytes);
        await HandleActionAsync(
            context,
            inject.ActionId,
            inject.Ciphertext,
            SessionCapability.SendInput,
            InputAuditKind.Inject,
            ownerBytes,
            ct);
    }

    private async Task HandlePermissionDecisionAsync(
        ParticipantPermissionDecisionMessage decision,
        RelayMessageContext context,
        CancellationToken ct)
    {
        var ownerMessage = new HostPermissionDecisionMessage(
            context.SessionId.Value.ToString(),
            decision.ActionId,
            decision.RequestId,
            decision.RequestGeneration,
            decision.Decision,
            context.UserId.Value.ToString(),
            context.DeviceId,
            decision.Nonce,
            decision.Ciphertext,
            decision.Signature);
        var ownerBytes = RelayOutbound.Encode(ownerMessage);
        EnsureTransformedOutputWithinLimit(
            "participant.permissionDecision",
            ownerBytes);
        var auditPayload = System.Text.Encoding.UTF8.GetString(ownerBytes);
        await HandleActionAwaitingHostResultAsync(
            context,
            decision.ActionId,
            decision.RequestId,
            decision.RequestGeneration,
            SessionCapability.ApproveDeny,
            InputAuditKind.PermissionDecision,
            auditPayload,
            ownerBytes,
            ct);
    }

    private async Task HandleResizeAsync(
        ParticipantResizeMessage resize,
        RelayMessageContext context,
        CancellationToken ct)
    {
        if (!context.IsOwnerParticipant)
        {
            await RejectActionAsync(
                context,
                resize.ActionId,
                InputAuditKind.Resize,
                resize.Ciphertext,
                ct);
            return;
        }
        var ownerMessage = new HostResizeMessage(
            context.SessionId.Value.ToString(),
            resize.ActionId,
            resize.Rows,
            resize.Cols,
            resize.WidthPixels,
            resize.HeightPixels,
            resize.CellWidthPixels,
            resize.CellHeightPixels,
            resize.Claim,
            resize.Nonce,
            resize.Ciphertext,
            context.UserId.Value.ToString(),
            context.DeviceId,
            resize.Signature);
        var ownerBytes = RelayOutbound.Encode(ownerMessage);
        EnsureTransformedOutputWithinLimit("participant.resize", ownerBytes);
        var auditPayload = $"{resize.Rows}x{resize.Cols}";
        await HandleActionAwaitingHostResultAsync(
            context,
            resize.ActionId,
            resize.ActionId,
            1,
            SessionCapability.Resize,
            InputAuditKind.Resize,
            auditPayload,
            ownerBytes,
            ct);
    }

    private async Task HandleFocusChangedAsync(
        ParticipantFocusChangedMessage focusChanged,
        RelayMessageContext context,
        CancellationToken ct)
    {
        var focused = focusChanged.Focused!.Value;
        var ownerMessage = new HostFocusChangedMessage(
            context.SessionId.Value.ToString(),
            focusChanged.ActionId,


            context.ConnectionId,
            focused,
            focusChanged.Nonce,
            focusChanged.Ciphertext,
            context.UserId.Value.ToString(),
            context.DeviceId,
            focusChanged.Signature);
        var ownerBytes = RelayOutbound.Encode(ownerMessage);
        EnsureTransformedOutputWithinLimit("participant.focusChanged", ownerBytes);
        var auditPayload = focused ? "focus" : "blur";
        await HandleActionAsync(
            context,
            focusChanged.ActionId,
            auditPayload,
            SessionCapability.Focus,
            InputAuditKind.FocusChange,
            ownerBytes,
            ct);
    }

    private async Task HandleStopAsync(
        ParticipantStopMessage stop,
        RelayMessageContext context,
        CancellationToken ct)
    {
        if (!context.IsOwnerParticipant)
        {
            await RejectActionAsync(
                context,
                stop.ActionId,
                InputAuditKind.Stop,
                stop.Ciphertext,
                ct);
            return;
        }
        var ownerBytes = RelayOutbound.Encode(new HostStopMessage(
            context.SessionId.Value.ToString(),
            stop.ActionId,
            stop.Nonce,
            stop.Ciphertext,
            context.UserId.Value.ToString(),
            context.DeviceId,
            stop.Signature));
        EnsureTransformedOutputWithinLimit("participant.stop", ownerBytes);
        await HandleActionAsync(
            context,
            stop.ActionId,
            stop.Ciphertext,
            SessionCapability.Stop,
            InputAuditKind.Stop,
            ownerBytes,
            ct);
    }

    private async Task HandleInterruptAsync(
        ParticipantInterruptMessage interrupt,
        RelayMessageContext context,
        CancellationToken ct)
    {
        if (!context.IsOwnerParticipant)
        {
            await RejectActionAsync(
                context,
                interrupt.ActionId,
                InputAuditKind.Interrupt,
                interrupt.Ciphertext,
                ct);
            return;
        }
        var ownerBytes = RelayOutbound.Encode(new HostInterruptMessage(
            context.SessionId.Value.ToString(),
            interrupt.ActionId,
            interrupt.Nonce,
            interrupt.Ciphertext,
            context.UserId.Value.ToString(),
            context.DeviceId,
            interrupt.Signature));
        EnsureTransformedOutputWithinLimit("participant.interrupt", ownerBytes);
        await HandleActionAsync(
            context,
            interrupt.ActionId,
            interrupt.Ciphertext,
            SessionCapability.Stop,
            InputAuditKind.Interrupt,
            ownerBytes,
            ct);
    }

}
