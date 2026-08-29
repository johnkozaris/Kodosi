using System.Collections.Concurrent;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class SessionSendQueues
{
    private readonly ConcurrentDictionary<string, RelayClientSendQueue> _participantQueues = new();
    private readonly Dictionary<string, SemanticReceiptRegistration> _semanticReceiptDestinations =
        new(StringComparer.Ordinal);
    private readonly Lock _hostQueueLock = new();
    private readonly Lock _lifecycleLock = new();
    private ChannelByteSendQueue? _hostQueue;
    private string? _hostConnectionId;
    private CloseReason? _hostAdmissionRejection;
    private CloseReason? _terminalCloseReason;

    public ChannelByteSendQueue? HostQueue
    {
        get
        {
            lock (_hostQueueLock)
            {
                return _hostQueue;
            }
        }
    }

    public ChannelByteSendQueue? TrySetHostQueue(
        string connectionId,
        byte[] initialMessage)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(connectionId);
        ArgumentNullException.ThrowIfNull(initialMessage);
        return SetHostQueueCore(connectionId, initialMessage);
    }

    private ChannelByteSendQueue? SetHostQueueCore(
        string? connectionId,
        byte[]? initialMessage)
    {
        ChannelByteSendQueue? previousQueue;
        var queue = new ChannelByteSendQueue();
        if (initialMessage is not null
            && queue.TryEnqueue(initialMessage) != ChannelByteSendQueueWriteOutcome.Enqueued)
        {
            queue.Complete(CloseReason.ServerError, discardPending: true);
            return null;
        }

        lock (_hostQueueLock)
        {
            if (_hostAdmissionRejection is not null)
            {
                queue.Complete(_hostAdmissionRejection.Value, discardPending: true);
                return null;
            }
            previousQueue = _hostQueue;
            _hostQueue = queue;
            _hostConnectionId = connectionId;
        }

        previousQueue?.Complete();
        return queue;
    }

    public void RejectHostAdmission(CloseReason reason)
    {
        ChannelByteSendQueue? queueToClear;
        lock (_hostQueueLock)
        {
            _hostAdmissionRejection ??= reason;
            queueToClear = _hostQueue;
            _hostQueue = null;
            _hostConnectionId = null;
        }

        queueToClear?.Complete(reason);
    }

    public bool ClearHostQueue(
        string? connectionId = null,
        CloseReason completionReason = CloseReason.ClosingNormal)
    {
        ChannelByteSendQueue? queueToClear;
        lock (_hostQueueLock)
        {
            if (connectionId is not null
                && !string.Equals(_hostConnectionId, connectionId, StringComparison.Ordinal))
            {
                return false;
            }

            queueToClear = _hostQueue;
            _hostQueue = null;
            _hostConnectionId = null;
        }

        queueToClear?.Complete(completionReason);
        return queueToClear is not null;
    }

    public RelayClientSendQueue AddPreparedParticipantQueue(
        string connectionId,
        AccessLevel accessLevel,
        Action<RelayClientSendQueue> prepareQueue,
        Func<CloseReason, WireMessage>? terminalMessageFactory = null,
        SemanticReceiptDestination? semanticReceiptDestination = null)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(connectionId);
        ArgumentNullException.ThrowIfNull(prepareQueue);

        var queue = new RelayClientSendQueue(accessLevel);
        lock (_lifecycleLock)
        {
            prepareQueue(queue);
            if (_terminalCloseReason is { } terminalCloseReason)
            {
                if (terminalMessageFactory is not null)
                {
                    _ = queue.TryCompleteWithControlMessage(
                        terminalMessageFactory(terminalCloseReason),
                        QueueCompletionCause.SessionEnd,
                        terminalCloseReason);
                }
                else
                {
                    queue.Complete(QueueCompletionCause.SessionEnd, closeReason: terminalCloseReason);
                }
            }

            _participantQueues.AddOrUpdate(
                connectionId,
                queue,
                (_, existing) =>
                {
                    existing.Complete();
                    return queue;
                });
            if (semanticReceiptDestination is { } destination)
            {
                _semanticReceiptDestinations[connectionId] =
                    new SemanticReceiptRegistration(destination, queue);
            }
            else
            {
                _semanticReceiptDestinations.Remove(connectionId);
            }
        }

        return queue;
    }

    public CloseReason? TerminalCloseReason
    {
        get
        {
            lock (_lifecycleLock)
            {
                return _terminalCloseReason;
            }
        }
    }

    public void MarkSessionEnded(CloseReason closeReason)
    {
        lock (_lifecycleLock)
        {
            _terminalCloseReason ??= closeReason;
        }
    }

    public int CompleteHostQueues(CloseReason completionReason)
    {
        ChannelByteSendQueue? hostQueue;
        lock (_hostQueueLock)
        {
            hostQueue = _hostQueue;
            _hostQueue = null;
            _hostConnectionId = null;
        }

        if (hostQueue is null)
        {
            return 0;
        }

        hostQueue.Complete(completionReason);
        return 1;
    }

    public bool RemoveParticipantQueueIfSame(
        string connectionId,
        RelayClientSendQueue expectedQueue)
    {
        RelayClientSendQueue? removedQueue = null;
        lock (_lifecycleLock)
        {
            if (_participantQueues.TryGetValue(connectionId, out var currentQueue)
                && ReferenceEquals(currentQueue, expectedQueue)
                && _participantQueues.TryRemove(
                    new KeyValuePair<string, RelayClientSendQueue>(
                        connectionId,
                        expectedQueue)))
            {
                removedQueue = expectedQueue;
                _semanticReceiptDestinations.Remove(connectionId);
            }
        }

        removedQueue?.Complete();
        return removedQueue is not null;
    }

    public IReadOnlyList<SemanticReceiptParticipant> GetSemanticReceiptParticipants(
        UserId requesterUserId,
        string requesterDeviceId)
    {
        lock (_lifecycleLock)
        {
            var participants = new List<SemanticReceiptParticipant>();
            foreach (var (connectionId, registration) in _semanticReceiptDestinations)
            {
                if (registration.Destination.RequesterUserId == requesterUserId
                    && string.Equals(
                        registration.Destination.RequesterDeviceId,
                        requesterDeviceId,
                        StringComparison.Ordinal)
                    && _participantQueues.TryGetValue(connectionId, out var currentQueue)
                    && ReferenceEquals(currentQueue, registration.Queue))
                {
                    participants.Add(new SemanticReceiptParticipant(
                        connectionId,
                        registration.Queue));
                }
            }

            return participants;
        }
    }

    public RelayClientSendQueue? GetParticipantQueue(string connectionId)
    {
        return _participantQueues.TryGetValue(connectionId, out var queue) ? queue : null;
    }

    public RelayClientSendQueueWriteOutcome TryEnqueueParticipantControlIfSame(
        string connectionId,
        RelayClientSendQueue expectedQueue,
        WireMessage message)
    {
        lock (_lifecycleLock)
        {
            if (!_participantQueues.TryGetValue(connectionId, out var currentQueue)
                || !ReferenceEquals(currentQueue, expectedQueue)
                || currentQueue.CompletionCause is not null)
            {
                return RelayClientSendQueueWriteOutcome.Closed;
            }

            return currentQueue.TryEnqueueControl(message);
        }
    }

    public bool CompleteParticipantIfSame(
        string connectionId,
        RelayClientSendQueue expectedQueue,
        QueueCompletionCause completionCause,
        bool discardPending,
        CloseReason? closeReason = null)
    {
        lock (_lifecycleLock)
        {
            if (!_participantQueues.TryGetValue(connectionId, out var currentQueue)
                || !ReferenceEquals(currentQueue, expectedQueue))
            {
                return false;
            }

            currentQueue.Complete(completionCause, discardPending, closeReason);
            return true;
        }
    }

    public bool CompleteParticipantWithControlIfSame(
        string connectionId,
        RelayClientSendQueue expectedQueue,
        WireMessage message,
        QueueCompletionCause completionCause,
        CloseReason? closeReason = null)
    {
        lock (_lifecycleLock)
        {
            if (!_participantQueues.TryGetValue(connectionId, out var currentQueue)
                || !ReferenceEquals(currentQueue, expectedQueue))
            {
                return false;
            }

            return currentQueue.TryCompleteWithControlMessage(
                message,
                completionCause,
                closeReason);
        }
    }

    public int ParticipantQueueCount
    {
        get
        {
            lock (_lifecycleLock)
            {
                return _participantQueues.Count;
            }
        }
    }

    public IReadOnlyList<KeyValuePair<string, RelayClientSendQueue>> GetParticipantQueueSnapshot()
    {
        lock (_lifecycleLock)
        {
            return _participantQueues.ToArray();
        }
    }

    public void CloseAll(
        CloseReason hostCompletionReason = CloseReason.ClosingNormal,
        QueueCompletionCause participantCompletionCause = QueueCompletionCause.Normal,
        CloseReason? participantCloseReason = null)
    {
        ChannelByteSendQueue? hostQueue;
        lock (_hostQueueLock)
        {
            hostQueue = _hostQueue;
            _hostQueue = null;
            _hostConnectionId = null;
        }

        RelayClientSendQueue[] participantQueues;
        lock (_lifecycleLock)
        {
            participantQueues = _participantQueues.Values.ToArray();
            _participantQueues.Clear();
            _semanticReceiptDestinations.Clear();
        }

        hostQueue?.Complete(hostCompletionReason);
        foreach (var queue in participantQueues)
        {
            queue.Complete(participantCompletionCause, closeReason: participantCloseReason);
        }
    }

    internal readonly record struct SemanticReceiptDestination(
        UserId RequesterUserId,
        string RequesterDeviceId);

    private readonly record struct SemanticReceiptRegistration(
        SemanticReceiptDestination Destination,
        RelayClientSendQueue Queue);

    internal readonly record struct SemanticReceiptParticipant(
        string ConnectionId,
        RelayClientSendQueue Queue);
}
