using System.Collections.Concurrent;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Observability;

using Kodosi.Host.Serialization;

namespace Kodosi.Host.Realtime;

internal sealed partial class SessionBroadcaster
{
    private static ChannelByteSendQueueWriteOutcome SendToHost(
        SessionSendQueues sendQueues,
        byte[] message) =>
        sendQueues.HostQueue?.TryEnqueue(message) ?? ChannelByteSendQueueWriteOutcome.Closed;

    public ChannelByteSendQueueWriteOutcome SendToHost(SessionId sessionId, byte[] message)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues))
        {
            return ChannelByteSendQueueWriteOutcome.Closed;
        }

        return SendToHost(sendQueues, message);
    }

    public bool IsHostAcceptancePending(SessionId sessionId)
    {
        var ports = _runtimes.TryGet(sessionId);
        return ports is not null
            && ports.Host.HostConnected
            && !ports.Host.HostReady;
    }

    public bool SendHostActionResultAckIfSame(
        SessionId sessionId,
        SessionSendQueues expectedQueues,
        Guid incarnationId,
        string actionId,
        string requesterUserId)
    {
        if (!ReferenceEquals(TryGetSession(sessionId), expectedQueues))
        {
            return false;
        }

        var message = RelayOutbound.Encode(new HostActionResultAckMessage(
            sessionId.Value.ToString(),
            incarnationId,
            actionId,
            requesterUserId));
        return SendToHost(expectedQueues, message)
            == ChannelByteSendQueueWriteOutcome.Enqueued;
    }

    public bool SendHostSemanticReceiptAckIfSame(
        SessionId sessionId,
        SessionSendQueues expectedQueues,
        Guid incarnationId,
        Guid requestId,
        string requesterUserId,
        string requesterDeviceId)
    {
        if (!ReferenceEquals(TryGetSession(sessionId), expectedQueues))
        {
            return false;
        }

        var message = RelayOutbound.Encode(new HostSemanticReceiptAckMessage(
            sessionId.Value.ToString(),
            incarnationId,
            requestId,
            requesterUserId,
            requesterDeviceId));
        return SendToHost(expectedQueues, message)
            == ChannelByteSendQueueWriteOutcome.Enqueued;
    }

    public void AcknowledgeHostFence(SessionId sessionId, string fenceId)
    {
        _fences.Acknowledge(sessionId, fenceId);
    }

    public void ReplayUnacknowledgedHostFences(SessionId sessionId)
    {
        _fences.ReplayUnacknowledged(sessionId);
    }

    public void ContinueUnacknowledgedHostFenceReplay(SessionId sessionId)
    {
        _fences.ContinueReplayUnacknowledged(sessionId);
    }

    public ChannelByteSendQueueWriteOutcome SendHostStreamDemand(
        SessionId sessionId,
        StreamDemandSnapshot demand,
        StreamDemandReason reason)
    {
        var sid = sessionId.Value.ToString();
        var message = new HostStreamDemandMessage(
            sid,
            demand.Required,
            demand.SharedParticipantCount,
            demand.OwnerParticipantCount,
            reason.ToWire());
        var bytes = RelayOutbound.Encode(message);
        return SendToHost(sessionId, bytes);
    }

    public void NotifyHostKeyDistributionRequested(SessionId sessionId)
    {
        _fences.NotifyKeyDistributionRequested(
            sessionId,
            hasLiveSessionQueues: _sessions.ContainsKey(sessionId));
    }


    public ChannelByteSendQueueWriteOutcome SendHostParticipantDisconnected(
    SessionId sessionId,
    string clientId)
    {
        var sid = sessionId.Value.ToString();
        var message = new HostParticipantDisconnectedMessage(sid, clientId);
        var bytes = RelayOutbound.Encode(message);
        return SendToHost(sessionId, bytes);
    }

    public void NotifyHostParticipantChanged(
        SessionId sessionId, int participantCount, string action, UserId participantUserId)
    {
        _fences.NotifyParticipantChanged(
            sessionId,
            participantCount,
            action,
            participantUserId,
            hasLiveSessionQueues: _sessions.ContainsKey(sessionId));
    }

    public void NotifyHostAccessRevoked(SessionId sessionId, UserId revokedUserId)
    {
        _fences.NotifyAccessRevoked(
            sessionId,
            revokedUserId,
            hasLiveSessionQueues: _sessions.ContainsKey(sessionId));
    }

    public void FlushPendingHostFences(SessionId sessionId)
    {
        _fences.FlushPending(sessionId);
    }


    public void ForceDisconnectHost(SessionId sessionId, CloseReason reason)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues)) return;
        sendQueues.CompleteHostQueues(reason);
        _logger.LogInformation(
            "host queue completed for session {SessionId} ({Reason})",
            sessionId,
            reason.ToWire());
    }

    private void RejectHostAdmission(SessionId sessionId, CloseReason reason)
    {
        if (!_sessions.TryGetValue(sessionId, out var sendQueues)) return;
        sendQueues.RejectHostAdmission(reason);
        _logger.LogError(
            "host admission rejected permanently for session {SessionId} ({Reason})",
            sessionId,
            reason.ToWire());
    }

}
