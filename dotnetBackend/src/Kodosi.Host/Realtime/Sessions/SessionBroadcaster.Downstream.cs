using Kodosi.Domain;
using Kodosi.Host.Observability;

using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed partial class SessionBroadcaster
{
    private void BroadcastToDownstreamClients(
        SessionId sessionId,
        SessionSendQueues sendQueues,
        WireMessage message)
    {
        IReadOnlyList<WireMessage>? resyncReplay = null;

        foreach (var (connectionId, downstreamQueue) in EnumerateDownstreamQueues(sendQueues))
        {
            var outcome = downstreamQueue.TryEnqueueFrame(message);
            if (outcome == RelayClientSendQueueWriteOutcome.Enqueued)
            {
                continue;
            }

            if (outcome == RelayClientSendQueueWriteOutcome.Overflow)
            {
                _metrics.RecordQueueOverflow(QueueOverflowLane.ParticipantFrame);
                resyncReplay ??= TryBuildResyncReplay(sessionId);

                var resyncOutcome = resyncReplay is null
                    ? RelayClientSendQueueWriteOutcome.Closed
                    : downstreamQueue.TryQueueTerminalReplay(
                        resyncReplay,
                        beginResync: true);
                if (resyncOutcome == RelayClientSendQueueWriteOutcome.Enqueued)
                {
                    _metrics.RecordResync();
                    _logger.LogWarning(
                        "Downstream client {ConnectionId} lagged on session {SessionId}; queued resync snapshot",
                        connectionId,
                        sessionId);
                }
                else
                {
                    if (resyncOutcome == RelayClientSendQueueWriteOutcome.Overflow)
                    {
                        _metrics.RecordControlQueueOverflowDisconnect();
                    }
                    else
                    {
                        downstreamQueue.Complete(QueueCompletionCause.Lagging, discardPending: true);
                    }

                    _logger.LogWarning(
                        "Downstream client {ConnectionId} lagged on session {SessionId} without a snapshot; disconnecting",
                        connectionId,
                        sessionId);
                }

                continue;
            }

            if (outcome == RelayClientSendQueueWriteOutcome.Disconnected)
            {
                _metrics.RecordQueueOverflow(QueueOverflowLane.ParticipantFrame);
                downstreamQueue.Complete(QueueCompletionCause.Lagging, discardPending: true);
                _logger.LogWarning(
                    "Downstream client {ConnectionId} remained behind after resync on session {SessionId}; disconnecting",
                    connectionId,
                    sessionId);
            }
        }

    }

    public bool BroadcastToDownstreamClientsIfSame(
        SessionId sessionId,
        SessionSendQueues expectedQueues,
        WireMessage message)
    {
        if (!ReferenceEquals(TryGetSession(sessionId), expectedQueues))
        {
            return false;
        }

        BroadcastToDownstreamClients(sessionId, expectedQueues, message);
        return true;
    }

    public bool SendToParticipantIfSame(
        SessionId sessionId,
        string connectionId,
        RelayClientSendQueue expectedQueue,
        WireMessage message)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues))
        {
            return false;
        }

        var outcome = sendQueues.TryEnqueueParticipantControlIfSame(
            connectionId,
            expectedQueue,
            message);
        return HandleControlEnqueueOutcome(sessionId, connectionId, outcome);
    }

    public bool SendSemanticReceiptToParticipantIfSame(
        SessionId sessionId,
        string connectionId,
        RelayClientSendQueue expectedQueue,
        Guid requestId,
        WireMessage message)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues)
            || !ReferenceEquals(sendQueues.GetParticipantQueue(connectionId), expectedQueue))
        {
            return false;
        }

        var outcome = expectedQueue.TryEnqueueSemanticReceipt(requestId, message);
        return HandleControlEnqueueOutcome(sessionId, connectionId, outcome);
    }

    public RelayClientSendQueue? TryGetParticipantQueue(SessionId sessionId, string connectionId)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues)) return null;
        return sendQueues.GetParticipantQueue(connectionId);
    }

    public void DisconnectParticipant(SessionId sessionId, string connectionId)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues)) return;

        var queue = sendQueues.GetParticipantQueue(connectionId);
        if (queue is null) return;

        var sid = sessionId.Value.ToString();
        var message = new SessionAccessRevokedMessage(sid);
        var bytes = RelayOutbound.Encode(message);
        _ = queue.TryCompleteWithControlMessage(
            WireMessage.Json(bytes),
            QueueCompletionCause.AccessRevokedCascade);
    }

    public bool DisconnectParticipantIfSame(
        SessionId sessionId,
        string connectionId,
        RelayClientSendQueue expectedQueue)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues)) return false;

        var sid = sessionId.Value.ToString();
        var message = new SessionAccessRevokedMessage(sid);
        var bytes = RelayOutbound.Encode(message);
        return sendQueues.CompleteParticipantWithControlIfSame(
            connectionId,
            expectedQueue,
            WireMessage.Json(bytes),
            QueueCompletionCause.AccessRevokedCascade);
    }

    public bool DisconnectParticipantForRefreshIfSame(
        SessionId sessionId,
        string connectionId,
        RelayClientSendQueue expectedQueue)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues)) return false;

        return sendQueues.CompleteParticipantIfSame(
            connectionId,
            expectedQueue,
            QueueCompletionCause.AccessRefresh,
            discardPending: true);
    }

    public void BroadcastSessionStatus(SessionId sessionId, SessionStatus state)
    {
        var sid = sessionId.Value.ToString();
        var message = new SessionStatusMessage(sid, state);
        var bytes = RelayOutbound.Encode(message);
        BroadcastControlToDownstreamClients(sessionId, WireMessage.Json(bytes));
    }

    public void BroadcastSessionEnded(SessionId sessionId, CloseReason reason)
    {
        if (_sessions.TryGetValue(sessionId, out var sendQueues))
        {
            sendQueues.MarkSessionEnded(reason);

            var sid = sessionId.Value.ToString();
            var message = new SessionEndedMessage(sid, reason.ToWire());
            var terminalMessage = WireMessage.Json(RelayOutbound.Encode(message));
            foreach (var (_, downstreamQueue) in EnumerateDownstreamQueues(sendQueues))
            {
                downstreamQueue.TryCompleteWithControlMessage(
                    terminalMessage,
                    QueueCompletionCause.SessionEnd,
                    reason);
            }
        }
    }

    public bool BroadcastKeyRotationToAllDownstreamClientsIfSame(
        SessionId sessionId,
        SessionSendQueues expectedQueues,
        WireMessage message) =>
        BroadcastKeyRotationToDownstreamClientsIfSame(
            sessionId,
            expectedQueues,
            message);

    private void BroadcastPendingPermissions(
        SessionId sessionId,
        SessionSendQueues sendQueues,
        WireMessage message)
    {
        foreach (var (connectionId, downstreamQueue) in EnumerateDownstreamQueues(sendQueues))
        {
            if (!downstreamQueue.CanReceivePendingPermissions)
            {
                continue;
            }

            var outcome = downstreamQueue.TryEnqueuePendingPermissionsSnapshot(message);
            if (outcome == RelayClientSendQueueWriteOutcome.Overflow)
            {
                _metrics.RecordQueueOverflow(QueueOverflowLane.Control);
                _logger.LogWarning(
                    "Downstream client {ConnectionId} rejected pending-permissions snapshot for session {SessionId}",
                    connectionId,
                    sessionId);
            }
        }

    }

    public bool BroadcastPendingPermissionsIfSame(
        SessionId sessionId,
        SessionSendQueues expectedQueues,
        WireMessage message)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues)
            || !ReferenceEquals(sendQueues, expectedQueues))
        {
            return false;
        }

        BroadcastPendingPermissions(sessionId, expectedQueues, message);
        return true;
    }
}
