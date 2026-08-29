using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;
using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed partial class RelayMessageProcessor
{
    private async Task HandleActionAwaitingHostResultAsync(
        RelayMessageContext context,
        string actionId,
        string requestId,
        long requestGeneration,
        SessionCapability requiredCapability,
        InputAuditKind auditKind,
        string auditPayload,
        byte[] ownerBytes,
        CancellationToken ct)
    {
        if (!RuntimeAccessGate.Allows(
                context.Capabilities,
                requiredCapability,
                context.Host.HostReady,
                context.Host.Status))
        {
            await RejectActionAsync(
                context,
                actionId,
                auditKind,
                auditPayload,
                ct,
                requestId,
                requestGeneration);
            return;
        }

        var claim = _dedupeCache.Claim(
            context.SessionId,
            context.UserId,
            actionId,
            requestId);
        if (claim.Kind == ActionDedupeClaimKind.Saturated)
        {
            SendActionResult(
                context,
                actionId,
                RelayActionStatus.Busy,
                requestId,
                requestGeneration);
            return;
        }
        if (claim.Kind == ActionDedupeClaimKind.Duplicate)
        {
            var canonicalTuple = new PermissionDecisionPendingTuple(
                context.SessionIncarnationId,
                context.SessionIncarnationGeneration,
                requestId,
                requestGeneration,
                context.DeviceId);
            PermissionDecisionAdmissionResult durable;
            try
            {
                durable = await _permissionDecisionAuditStore.AdmitAsync(
                    context.SessionId,
                    context.UserId,
                    actionId,
                    auditPayload,
                    canonicalTuple,
                    ct);
            }
            catch (Exception ex) when (ex is not OperationCanceledException || !ct.IsCancellationRequested)
            {
                _logger.LogError(
                    ex,
                    "Failed to read duplicate host-applied action {ActionId}",
                    actionId);
                return;
            }
            if (durable.Outcome == PermissionDecisionAdmissionOutcome.AcceptedDuplicate)
            {
                SendActionResult(
                    context,
                    actionId,
                    RelayActionStatus.Accepted,
                    durable.CanonicalTuple!.Value.RequestId,
                    durable.CanonicalTuple.Value.RequestGeneration);
                return;
            }
            if (durable.Outcome == PermissionDecisionAdmissionOutcome.RejectedDuplicate)
            {
                SendActionResult(
                    context,
                    actionId,
                    RelayActionStatus.Rejected,
                    durable.CanonicalTuple!.Value.RequestId,
                    durable.CanonicalTuple.Value.RequestGeneration);
                return;
            }
            return;
        }

        var pendingTuple = new PermissionDecisionPendingTuple(
            context.SessionIncarnationId,
            context.SessionIncarnationGeneration,
            requestId,
            requestGeneration,
            context.DeviceId);
        PermissionDecisionAdmissionResult audit;
        try
        {
            audit = await _permissionDecisionAuditStore.AdmitAsync(
                context.SessionId,
                context.UserId,
                actionId,
                auditPayload,
                pendingTuple,
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
                "Failed to durably admit host-applied action {ActionId}",
                actionId);
            CompleteAndSendActionResult(
                context,
                actionId,
                claim.LeaseId,
                RelayActionStatus.Busy,
                requestId,
                requestGeneration);
            return;
        }
        switch (audit.Outcome)
        {
            case PermissionDecisionAdmissionOutcome.AcceptedDuplicate:
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    RelayActionStatus.Accepted,
                    audit.CanonicalTuple!.Value.RequestId,
                    audit.CanonicalTuple.Value.RequestGeneration);
                return;
            case PermissionDecisionAdmissionOutcome.RejectedDuplicate:
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    RelayActionStatus.Rejected,
                    audit.CanonicalTuple!.Value.RequestId,
                    audit.CanonicalTuple.Value.RequestGeneration);
                return;
            case PermissionDecisionAdmissionOutcome.PendingDuplicate:



                break;
            case PermissionDecisionAdmissionOutcome.Rearmed:
                break;
            case PermissionDecisionAdmissionOutcome.Conflict:
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    RelayActionStatus.Rejected,
                    requestId,
                    requestGeneration);
                return;
            case PermissionDecisionAdmissionOutcome.Failed:
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    RelayActionStatus.Busy,
                    requestId,
                    requestGeneration);
                return;
        }

        var dispatch = await context.ActionAuthority.TryDispatchAsync(
            requiredCapability,
            () => _broadcaster.SendToHost(context.SessionId, ownerBytes),
            ct);
        if (!dispatch.Authorized
            || dispatch.SendOutcome != ChannelByteSendQueueWriteOutcome.Enqueued)
        {
            try
            {
                await _permissionDecisionAuditStore.MarkDispatchFailedAsync(
                    context.SessionId,
                    context.UserId,
                    actionId,
                    pendingTuple,
                    ct);
            }
            catch (Exception ex) when (ex is not OperationCanceledException || !ct.IsCancellationRequested)
            {
                _logger.LogError(
                    ex,
                    "Failed to mark host-applied action dispatch failure {ActionId}",
                    actionId);
            }
            CompleteAndSendActionResult(
                context,
                actionId,
                claim.LeaseId,
                dispatch.SendOutcome == ChannelByteSendQueueWriteOutcome.Full
                    ? RelayActionStatus.Busy
                    : RelayActionStatus.Rejected,
                requestId,
                requestGeneration);
        }
    }

    private async Task HandleActionAsync(
        RelayMessageContext context,
        string actionId,
        string payload,
        SessionCapability requiredCapability,
        InputAuditKind auditKind,
        byte[] ownerBytes,
        CancellationToken ct,
        string? requestId = null)
    {
        var claim = _dedupeCache.Claim(
            context.SessionId,
            context.UserId,
            actionId,
            requestId);
        if (await TryHandleNonNewClaimAsync(
                context,
                actionId,
                auditKind,
                payload,
                claim,
                ct,
                requestId))
        {
            return;
        }

        try
        {
            if (!RuntimeAccessGate.Allows(
                    context.Capabilities,
                    requiredCapability,
                    context.Host.HostReady,
                    context.Host.Status))
            {
                var rejectedStatus = await PersistRejectedActionAsync(
                    context,
                    actionId,
                    auditKind,
                    payload,
                    ct);
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    rejectedStatus,
                    requestId);
                return;
            }





            var participantQueue = _broadcaster.TryGetParticipantQueue(
                context.SessionId,
                context.ConnectionId);
            if (participantQueue is null || participantQueue.CompletionCause is not null)
            {
                var rejectedStatus = await PersistRejectedActionAsync(
                    context,
                    actionId,
                    auditKind,
                    payload,
                    ct);
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    rejectedStatus,
                    requestId);
                return;
            }

            var auditOutcome = await AppendAuditAsync(
                context,
                actionId,
                auditKind,
                payload,
                InputAuditStatus.Pending,
                ct);
            if (auditOutcome == AuditAppendOutcome.AlreadyExists
                && !await _auditWriter.TryRearmAsync(
                    context.SessionId,
                    context.UserId,
                    actionId,
                    InputAuditStatus.Pending,
                    ct))
            {
                var replayStatus = await GetDurableReplayStatusAsync(
                    context,
                    actionId,
                    ct);
                await RecordDuplicateAuditAsync(context, actionId, auditKind, payload, ct);
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    replayStatus,
                    requestId);
                return;
            }
            if (auditOutcome == AuditAppendOutcome.Failed)
            {
                _metrics.RecordActionRejected();
                _metrics.RecordQueueOverflow(QueueOverflowLane.Audit);
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    RelayActionStatus.Busy,
                    requestId);
                return;
            }

            var dispatch = await context.ActionAuthority.TryDispatchAsync(
                requiredCapability,
                () => _broadcaster.SendToHost(
                    context.SessionId,
                    ownerBytes),
                ct);
            if (!dispatch.Authorized)
            {
                _metrics.RecordActionRejected();
                await TryUpdateAuditStatusAsync(
                    context,
                    actionId,
                    auditKind,
                    InputAuditStatus.Rejected,
                    ct);
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    RelayActionStatus.Rejected,
                    requestId);
                return;
            }

            var sendOutcome = dispatch.SendOutcome;

            if (sendOutcome == ChannelByteSendQueueWriteOutcome.Enqueued)
            {
                await TryUpdateAuditStatusAsync(
                    context,
                    actionId,
                    auditKind,
                    InputAuditStatus.Dispatched,
                    ct);
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    RelayActionStatus.Accepted,
                    requestId);
                return;
            }

            if (sendOutcome == ChannelByteSendQueueWriteOutcome.Full)
            {
                _metrics.RecordQueueOverflow(QueueOverflowLane.Host);
                _metrics.RecordActionRejected();
                await TryUpdateAuditStatusAsync(
                    context,
                    actionId,
                    auditKind,
                    InputAuditStatus.Failed,
                    ct);
                CompleteAndSendActionResult(
                    context,
                    actionId,
                    claim.LeaseId,
                    RelayActionStatus.Busy,
                    requestId);
                return;
            }

            _metrics.RecordActionRejected();
            await TryUpdateAuditStatusAsync(
                context,
                actionId,
                auditKind,
                InputAuditStatus.Failed,
                ct);
            CompleteAndSendActionResult(
                context,
                actionId,
                claim.LeaseId,
                RelayActionStatus.Rejected,
                requestId);
        }
        catch
        {
            _dedupeCache.Complete(
                context.SessionId,
                context.UserId,
                actionId,
                claim.LeaseId,
                ActionDedupeFinalOutcome.Busy);
            throw;
        }
    }

    private async Task RejectActionAsync(
        RelayMessageContext context,
        string actionId,
        InputAuditKind auditKind,
        string payload,
        CancellationToken ct,
        string? requestId = null,
        long? requestGeneration = null)
    {
        var claim = _dedupeCache.Claim(context.SessionId, context.UserId, actionId);
        if (await TryHandleNonNewClaimAsync(
                context,
                actionId,
                auditKind,
                payload,
                claim,
                ct,
                requestId))
        {
            return;
        }

        try
        {
            var status = await PersistRejectedActionAsync(
                context,
                actionId,
                auditKind,
                payload,
                ct);
            CompleteAndSendActionResult(
                context,
                actionId,
                claim.LeaseId,
                status,
                requestId,
                requestGeneration);
        }
        catch
        {
            _dedupeCache.Complete(
                context.SessionId,
                context.UserId,
                actionId,
                claim.LeaseId,
                ActionDedupeFinalOutcome.Busy);
            throw;
        }
    }

    private async Task<RelayActionStatus> PersistRejectedActionAsync(
        RelayMessageContext context,
        string actionId,
        InputAuditKind auditKind,
        string payload,
        CancellationToken ct)
    {
        _metrics.RecordActionRejected();
        var outcome = await AppendAuditAsync(
            context,
            actionId,
            auditKind,
            payload,
            InputAuditStatus.Rejected,
            ct);
        if (outcome == AuditAppendOutcome.AlreadyExists)
        {
            if (await _auditWriter.TryRearmAsync(
                    context.SessionId,
                    context.UserId,
                    actionId,
                    InputAuditStatus.Rejected,
                    ct))
            {
                return RelayActionStatus.Rejected;
            }
            var replayStatus = await GetDurableReplayStatusAsync(
                context,
                actionId,
                ct);
            await RecordDuplicateAuditAsync(context, actionId, auditKind, payload, ct);
            return replayStatus;
        }

        return RelayActionStatus.Rejected;
    }

    private async Task<bool> TryHandleNonNewClaimAsync(
        RelayMessageContext context,
        string actionId,
        InputAuditKind auditKind,
        string payload,
        ActionDedupeClaim claim,
        CancellationToken ct,
        string? requestId,
        long? requestGeneration = null)
    {
        if (claim.Kind == ActionDedupeClaimKind.Saturated)
        {
            _metrics.RecordQueueOverflow(QueueOverflowLane.Audit);
            SendActionResult(
                context,
                actionId,
                RelayActionStatus.Busy,
                claim.OriginalRequestId ?? requestId,
                requestGeneration);
            return true;
        }
        if (claim.Kind != ActionDedupeClaimKind.Duplicate)
        {
            return false;
        }

        var shouldRecordDuplicate = _dedupeCache.TryClaimAuditSlot(
            context.SessionId,
            context.UserId,
            actionId);
        var outcome = await claim.OriginalOutcome!.WaitAsync(ct);
        if (outcome != ActionDedupeFinalOutcome.Busy && shouldRecordDuplicate)
        {
            await RecordDuplicateAuditAsync(context, actionId, auditKind, payload, ct);
        }
        SendActionResult(
            context,
            actionId,
            outcome switch
            {
                ActionDedupeFinalOutcome.Accepted => RelayActionStatus.Accepted,
                ActionDedupeFinalOutcome.Duplicate => RelayActionStatus.Duplicate,
                ActionDedupeFinalOutcome.Rejected => RelayActionStatus.Rejected,
                _ => RelayActionStatus.Busy,
            },
            claim.OriginalRequestId ?? requestId,
            requestGeneration);
        return true;
    }

    private void CompleteAndSendActionResult(
        RelayMessageContext context,
        string actionId,
        long leaseId,
        RelayActionStatus status,
        string? requestId = null,
        long? requestGeneration = null)
    {
        _dedupeCache.Complete(
            context.SessionId,
            context.UserId,
            actionId,
            leaseId,
            status switch
            {
                RelayActionStatus.Accepted => ActionDedupeFinalOutcome.Accepted,
                RelayActionStatus.Duplicate => ActionDedupeFinalOutcome.Duplicate,
                RelayActionStatus.Rejected => ActionDedupeFinalOutcome.Rejected,
                _ => ActionDedupeFinalOutcome.Busy,
            });
        SendActionResult(context, actionId, status, requestId, requestGeneration);
    }

    private async Task<RelayActionStatus> GetDurableReplayStatusAsync(
        RelayMessageContext context,
        string actionId,
        CancellationToken ct)
    {
        var status = await _auditWriter.GetStatusAsync(
            context.SessionId,
            context.UserId,
            actionId,
            ct);
        return status switch
        {
            InputAuditStatus.Dispatched => RelayActionStatus.Accepted,
            InputAuditStatus.Rejected => RelayActionStatus.Rejected,
            InputAuditStatus.Pending or InputAuditStatus.Failed => RelayActionStatus.Busy,
            _ => RelayActionStatus.Duplicate,
        };
    }

    private Task<AuditAppendOutcome> AppendAuditAsync(
        RelayMessageContext context,
        string actionId,
        InputAuditKind kind,
        string payload,
        InputAuditStatus status,
        CancellationToken ct)
        => _auditWriter.AppendAsync(
                context.SessionId,
                context.UserId,
                actionId,
                kind,
                payload,
                status,
                ct);

    private async Task RecordDuplicateAuditAsync(
        RelayMessageContext context,
        string actionId,
        InputAuditKind kind,
        string payload,
        CancellationToken ct)
    {
        if (!await _auditWriter.RecordDuplicateAsync(
                context.SessionId,
                context.UserId,
                actionId,
                kind,
                payload,
                ct))
        {
            _metrics.RecordQueueOverflow(QueueOverflowLane.Audit);
        }
    }

    private async Task TryUpdateAuditStatusAsync(
        RelayMessageContext context,
        string actionId,
        InputAuditKind kind,
        InputAuditStatus status,
        CancellationToken ct)
    {
        if (!await _auditWriter.UpdateStatusAsync(
                context.SessionId,
                context.UserId,
                actionId,
                status,
                ct))
        {
            _metrics.RecordQueueOverflow(QueueOverflowLane.Audit);
        }
    }

    private void SendActionResult(
        RelayMessageContext context,
        string actionId,
        RelayActionStatus status,
        string? requestId = null,
        long? requestGeneration = null)
    {
        var message = new ActionResultMessage(
            context.SessionId.Value.ToString(),
            actionId,
            status,
            requestId,
            requestGeneration);
        var bytes = RelayOutbound.Encode(message);
        var wireMessage = WireMessage.Json(bytes);
        if (context.ParticipantQueue is not { } expectedQueue)
        {
            return;
        }
        _ = _broadcaster.SendToParticipantIfSame(
            context.SessionId,
            context.ConnectionId,
            expectedQueue,
            wireMessage);
    }
}
