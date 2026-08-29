using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal enum RelayClientSendQueueWriteOutcome
{
    Enqueued,
    SkippedForReconnect,
    Overflow,
    Disconnected,
    Closed,
}

internal sealed class RelayClientSendQueue(AccessLevel accessLevel = AccessLevel.View)
{
    private const int MaxControlBacklog = 16;
    private const int MaxTerminalReplayBacklog = 260;
    private const int MaxFrameBacklog = 64;
    private const int MaxControlBacklogBytes = RelayMessageLimits.TerminalCheckpointFrameMaxBytes;
    internal const int MaxTerminalReplayBacklogBytes =
        RelayMessageLimits.TerminalCheckpointFrameMaxBytes
        + RelayMessageLimits.TerminalPresentationFrameMaxBytes
        + (4 * RelayMessageLimits.OuterMessageMaxBytes)
        + RelayMessageLimits.OuterMessageMaxBytes;
    internal const int MaxFrameBacklogBytes = RelayMessageLimits.TerminalCheckpointFrameMaxBytes;
    internal const int MaxTerminalMessageBytes = 64 * 1024;

    private readonly Lock _lock = new();
    private readonly Queue<PriorityMessage> _controlMessages = new();
    private readonly Queue<WireMessage> _frameMessages = new();
    private WireMessage? _pendingPermissionsMessage;
    private readonly Queue<SemanticReceiptDelivery> _semanticReceiptAdmission = new();
    private readonly HashSet<Guid> _semanticReceiptAdmissionRequestIds = [];
    private readonly SemaphoreSlim _available = new(0);
    private readonly CancellationTokenSource _completionCts = new();
    private bool _completed;
    private bool _resyncInProgress;
    private int _controlMessageCount;
    private int _terminalReplayMessageCount;
    private int _controlBytes;
    private int _terminalReplayBytes;
    private int _frameBytes;
    private int _semanticReceiptAdmissionBytes;
    private bool _terminalMessagePending;
    private bool _semanticReceiptAdmissionOpen;
    private bool _isOwnerParticipant;
    private QueueCompletionCause? _completionCause;
    private CloseReason? _completionCloseReason;

    public AccessLevel AccessLevel { get; } = accessLevel;
    public bool CanReceivePendingPermissions =>
        _isOwnerParticipant
        || SessionCapabilities.FromAccess(AccessLevel).CanReceivePendingPermissions;
    public CancellationToken CompletionToken => _completionCts.Token;

    public QueueCompletionCause? CompletionCause
    {
        get
        {
            lock (_lock)
            {
                return _completionCause;
            }
        }
    }

    public CloseReason? CompletionCloseReason
    {
        get
        {
            lock (_lock)
            {
                return _completionCloseReason;
            }
        }
    }

    public RelayClientSendQueueWriteOutcome TryEnqueueControl(WireMessage message)
    {
        var outcome = RelayClientSendQueueWriteOutcome.Enqueued;
        lock (_lock)
        {
            outcome = TryEnqueueControlUnsafe(message);
        }

        _available.Release();
        return outcome;
    }

    public RelayClientSendQueueWriteOutcome TryEnqueueKeyRotation(WireMessage message)
    {
        RelayClientSendQueueWriteOutcome outcome;
        lock (_lock)
        {
            PurgeEncryptedFramesUnsafe();
            outcome = TryEnqueueControlUnsafe(message);
        }

        _available.Release();
        return outcome;
    }

    public void MarkOwnerParticipant()
    {
        lock (_lock)
        {
            _isOwnerParticipant = true;
        }
    }

    public RelayClientSendQueueWriteOutcome TryEnqueuePendingPermissionsSnapshot(
        WireMessage message)
    {
        var signal = false;
        lock (_lock)
        {
            if (_completed)
            {
                return RelayClientSendQueueWriteOutcome.Closed;
            }
            if (MessageBytes(message) > RelayMessageLimits.OuterMessageMaxBytes)
            {
                return RelayClientSendQueueWriteOutcome.Overflow;
            }
            signal = _pendingPermissionsMessage is null;
            _pendingPermissionsMessage = message;
        }
        if (signal)
        {
            _available.Release();
        }
        return RelayClientSendQueueWriteOutcome.Enqueued;
    }

    public bool BeginSemanticReceiptAdmission()
    {
        lock (_lock)
        {
            if (_completed || _semanticReceiptAdmissionOpen)
            {
                return false;
            }

            _semanticReceiptAdmissionOpen = true;
            return true;
        }
    }

    public RelayClientSendQueueWriteOutcome TryEnqueueSemanticReceipt(
        Guid requestId,
        WireMessage message)
    {
        RelayClientSendQueueWriteOutcome outcome;
        lock (_lock)
        {
            if (_completed)
            {
                return RelayClientSendQueueWriteOutcome.Closed;
            }

            if (_semanticReceiptAdmissionOpen)
            {
                if (_semanticReceiptAdmissionRequestIds.Contains(requestId))
                {
                    return RelayClientSendQueueWriteOutcome.Enqueued;
                }
                if (_semanticReceiptAdmission.Count >= MaxControlBacklog
                    || _semanticReceiptAdmissionBytes + MessageBytes(message)
                        > MaxControlBacklogBytes)
                {
                    CompleteForLaggingClient();
                    outcome = RelayClientSendQueueWriteOutcome.Overflow;
                }
                else
                {
                    _semanticReceiptAdmissionRequestIds.Add(requestId);
                    _semanticReceiptAdmission.Enqueue(new SemanticReceiptDelivery(
                        requestId,
                        message));
                    _semanticReceiptAdmissionBytes += MessageBytes(message);
                    return RelayClientSendQueueWriteOutcome.Enqueued;
                }
            }
            else
            {
                outcome = TryEnqueueControlUnsafe(message);
            }
        }

        _available.Release();
        return outcome;
    }

    public RelayClientSendQueueWriteOutcome CompleteSemanticReceiptAdmission(
        IReadOnlyList<SemanticReceiptDelivery> replay)
    {
        ArgumentNullException.ThrowIfNull(replay);
        var outcome = RelayClientSendQueueWriteOutcome.Enqueued;
        var queuedAny = false;
        lock (_lock)
        {
            if (_completed || !_semanticReceiptAdmissionOpen)
            {
                return _completed
                    ? RelayClientSendQueueWriteOutcome.Closed
                    : RelayClientSendQueueWriteOutcome.Disconnected;
            }

            var bufferedLive = _semanticReceiptAdmission.ToArray();
            _semanticReceiptAdmission.Clear();
            _semanticReceiptAdmissionRequestIds.Clear();
            _semanticReceiptAdmissionBytes = 0;
            foreach (var receipt in replay.Concat(bufferedLive))
            {
                if (_semanticReceiptAdmissionRequestIds.Contains(receipt.RequestId))
                {
                    continue;
                }
                if (_semanticReceiptAdmission.Count >= MaxControlBacklog
                    || _semanticReceiptAdmissionBytes + MessageBytes(receipt.Message)
                        > MaxControlBacklogBytes)
                {
                    CompleteForLaggingClient();
                    outcome = RelayClientSendQueueWriteOutcome.Overflow;
                    break;
                }
                _semanticReceiptAdmissionRequestIds.Add(receipt.RequestId);
                _semanticReceiptAdmission.Enqueue(receipt);
                _semanticReceiptAdmissionBytes += MessageBytes(receipt.Message);
            }

            while (outcome == RelayClientSendQueueWriteOutcome.Enqueued
                && _semanticReceiptAdmission.TryDequeue(out var receipt))
            {
                _semanticReceiptAdmissionBytes -= MessageBytes(receipt.Message);
                outcome = TryEnqueueControlUnsafe(receipt.Message);
                if (outcome != RelayClientSendQueueWriteOutcome.Enqueued)
                {
                    break;
                }
                queuedAny = true;
            }
            _semanticReceiptAdmission.Clear();
            _semanticReceiptAdmissionRequestIds.Clear();
            _semanticReceiptAdmissionBytes = 0;
            _semanticReceiptAdmissionOpen = false;
        }

        if (queuedAny || outcome != RelayClientSendQueueWriteOutcome.Enqueued)
        {
            _available.Release();
        }
        return outcome;
    }

    public void CancelSemanticReceiptAdmission()
    {
        lock (_lock)
        {
            _semanticReceiptAdmission.Clear();
            _semanticReceiptAdmissionRequestIds.Clear();
            _semanticReceiptAdmissionBytes = 0;
            _semanticReceiptAdmissionOpen = false;
        }
    }

    internal readonly record struct SemanticReceiptDelivery(
        Guid RequestId,
        WireMessage Message);

    public RelayClientSendQueueWriteOutcome TryEnqueueFrame(WireMessage message)
    {
        lock (_lock)
        {
            if (_completed)
            {
                return RelayClientSendQueueWriteOutcome.Closed;
            }

            if (_frameMessages.Count < MaxFrameBacklog
                && _frameBytes + MessageBytes(message) <= MaxFrameBacklogBytes)
            {
                _frameMessages.Enqueue(message);
                _frameBytes += MessageBytes(message);
                _available.Release();
                return RelayClientSendQueueWriteOutcome.Enqueued;
            }

            return _resyncInProgress
                ? RelayClientSendQueueWriteOutcome.Disconnected
                : RelayClientSendQueueWriteOutcome.Overflow;
        }
    }

    public RelayClientSendQueueWriteOutcome TryQueueTerminalReplay(
        IReadOnlyList<WireMessage> messages,
        bool beginResync = false)
    {
        ArgumentNullException.ThrowIfNull(messages);
        var outcome = RelayClientSendQueueWriteOutcome.Enqueued;
        lock (_lock)
        {
            if (_completed || (beginResync && _resyncInProgress))
            {
                return _completed
                    ? RelayClientSendQueueWriteOutcome.Closed
                    : RelayClientSendQueueWriteOutcome.Disconnected;
            }

            var replayBytes = 0;
            foreach (var message in messages)
            {
                replayBytes = checked(replayBytes + MessageBytes(message));
            }
            if (_terminalReplayBytes + replayBytes > MaxTerminalReplayBacklogBytes
                || _terminalReplayMessageCount + messages.Count
                    > MaxTerminalReplayBacklog)
            {
                CompleteForLaggingClient();
                outcome = RelayClientSendQueueWriteOutcome.Overflow;
            }
            else
            {
                if (beginResync)
                {
                    ClearFramesUnsafe();
                    _resyncInProgress = true;
                }
                foreach (var message in messages)
                {
                    _controlMessages.Enqueue(new PriorityMessage(message, IsReplay: true));
                }
                _terminalReplayMessageCount += messages.Count;
                _terminalReplayBytes += replayBytes;
            }
        }

        if (messages.Count > 0 || outcome != RelayClientSendQueueWriteOutcome.Enqueued)
        {
            _available.Release();
        }
        return outcome;
    }

    internal async Task<RelayClientQueueDelivery?> ReadForSendAsync(
        CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            lock (_lock)
            {
                if (_controlMessages.Count > 0)
                {
                    var queued = _controlMessages.Dequeue();
                    var message = queued.Message;
                    if (queued.IsReplay)
                    {
                        _terminalReplayMessageCount--;
                        _terminalReplayBytes -= MessageBytes(message);
                    }
                    else
                    {
                        _controlMessageCount--;
                        _controlBytes -= MessageBytes(message);
                    }
                    var isTerminal = _terminalMessagePending;
                    _terminalMessagePending = false;
                    UpdateResyncStateAfterDequeue();
                    return new RelayClientQueueDelivery(
                        message,
                        isTerminal);
                }

                if (_pendingPermissionsMessage is { } pendingPermissions)
                {
                    _pendingPermissionsMessage = null;
                    return new RelayClientQueueDelivery(
                        pendingPermissions,
                        IsTerminal: false);
                }

                if (_frameMessages.Count > 0)
                {
                    var message = _frameMessages.Dequeue();
                    _frameBytes -= MessageBytes(message);
                    UpdateResyncStateAfterDequeue();
                    return new RelayClientQueueDelivery(
                        message,
                        IsTerminal: false);
                }

                if (_completed)
                {
                    return null;
                }
            }

            await _available.WaitAsync(ct);
        }

        return null;
    }

    public void Complete(
        QueueCompletionCause completionCause = QueueCompletionCause.Normal,
        bool discardPending = false,
        CloseReason? closeReason = null)
    {
        lock (_lock)
        {
            if (_completed)
            {
                return;
            }
            if (discardPending)
            {
                ClearAllUnsafe();
            }

            _completed = true;
            _completionCause ??= completionCause;
            _completionCloseReason ??= closeReason;
        }

        _available.Release();
        _completionCts.Cancel();
    }

    public bool TryCompleteWithControlMessage(
        WireMessage message,
        QueueCompletionCause completionCause = QueueCompletionCause.Normal,
        CloseReason? closeReason = null)
    {
        var queuedTerminal = false;
        lock (_lock)
        {
            if (_completed)
            {
                return false;
            }
            ClearAllUnsafe();
            if (MessageBytes(message) <= MaxTerminalMessageBytes)
            {
                _controlMessages.Enqueue(new PriorityMessage(message, IsReplay: false));
                _controlMessageCount = 1;
                _controlBytes = MessageBytes(message);
                _terminalMessagePending = true;
                _resyncInProgress = false;
                queuedTerminal = true;
            }
            _completed = true;
            _completionCause ??= completionCause;
            _completionCloseReason ??= closeReason;
        }

        _available.Release();
        _completionCts.Cancel();
        return queuedTerminal;
    }

    public bool TryTakePendingTerminalMessage(out WireMessage message)
    {
        lock (_lock)
        {
            if (!_terminalMessagePending
                || !_controlMessages.TryDequeue(out var queued))
            {
                message = default;
                return false;
            }

            message = queued.Message;
            if (queued.IsReplay)
            {
                _terminalReplayMessageCount--;
                _terminalReplayBytes -= MessageBytes(message);
            }
            else
            {
                _controlMessageCount--;
                _controlBytes -= MessageBytes(message);
            }
            _terminalMessagePending = false;
            return true;
        }
    }

    internal readonly record struct RelayClientQueueDelivery(
        WireMessage Message,
        bool IsTerminal);

    private readonly record struct PriorityMessage(
        WireMessage Message,
        bool IsReplay);

    public bool TryExecuteWhileOpen<T>(Func<T> action, out T result)
    {
        ArgumentNullException.ThrowIfNull(action);
        lock (_lock)
        {
            if (_completed)
            {
                result = default!;
                return false;
            }

            result = action();
            return true;
        }
    }

    private RelayClientSendQueueWriteOutcome TryEnqueueControlUnsafe(WireMessage message)
    {
        if (_completed)
        {
            return RelayClientSendQueueWriteOutcome.Closed;
        }

        if (_controlMessageCount >= MaxControlBacklog
            || _controlBytes + MessageBytes(message) > MaxControlBacklogBytes)
        {


            CompleteForLaggingClient();
            return RelayClientSendQueueWriteOutcome.Overflow;
        }

        _controlMessages.Enqueue(new PriorityMessage(message, IsReplay: false));
        _controlMessageCount++;
        _controlBytes += MessageBytes(message);
        return RelayClientSendQueueWriteOutcome.Enqueued;
    }

    private void UpdateResyncStateAfterDequeue()
    {
        if (_resyncInProgress &&
            _controlMessages.Count == 0 &&
            _frameMessages.Count == 0)
        {
            _resyncInProgress = false;
        }
    }

    private void CompleteForLaggingClient()
    {
        ClearAllUnsafe();

        _completed = true;
        _resyncInProgress = false;
        _completionCause ??= QueueCompletionCause.Lagging;
        _completionCts.Cancel();
    }

    private static int MessageBytes(WireMessage message) =>
        message.Payload.Length;

    private void ClearFramesUnsafe()
    {
        _frameMessages.Clear();
        _frameBytes = 0;
    }

    private void PurgeEncryptedFramesUnsafe()
    {
        var retainedControls = new Queue<PriorityMessage>();
        while (_controlMessages.TryDequeue(out var queued))
        {
            if (queued.Message.Kind != WireMessageKind.EncryptedBinary)
            {
                retainedControls.Enqueue(queued);
                continue;
            }

            if (queued.IsReplay)
            {
                _terminalReplayMessageCount--;
                _terminalReplayBytes -= MessageBytes(queued.Message);
            }
            else
            {
                _controlMessageCount--;
                _controlBytes -= MessageBytes(queued.Message);
            }
        }
        while (retainedControls.TryDequeue(out var retained))
        {
            _controlMessages.Enqueue(retained);
        }

        ClearFramesUnsafe();
        _pendingPermissionsMessage = null;
        UpdateResyncStateAfterDequeue();
    }

    private void ClearAllUnsafe()
    {
        _controlMessages.Clear();
        _frameMessages.Clear();
        _pendingPermissionsMessage = null;
        _controlMessageCount = 0;
        _terminalReplayMessageCount = 0;
        _controlBytes = 0;
        _terminalReplayBytes = 0;
        _frameBytes = 0;
        _terminalMessagePending = false;
        _semanticReceiptAdmission.Clear();
        _semanticReceiptAdmissionRequestIds.Clear();
        _semanticReceiptAdmissionOpen = false;
    }
}
