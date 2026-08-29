using System.Threading.Channels;

namespace Kodosi.Host.Realtime;

internal enum ChannelByteSendQueueWriteOutcome
{
    Enqueued,
    Full,
    Closed,
}

internal sealed class ChannelByteSendQueue
{
    internal const int DefaultMaxBacklogBytes = 8 * 1024 * 1024;
    private readonly Channel<byte[]> _messages;
    private readonly Lock _completionLock = new();
    private readonly CancellationTokenSource _completionCts = new();
    private readonly int _maxBacklogBytes;
    private int _queuedBytes;
    private int _completed;
    private int _discardPending;
    private CloseReason _completionReason = CloseReason.ClosingNormal;

    public ChannelByteSendQueue(
        int maxBacklog = 256,
        int maxBacklogBytes = DefaultMaxBacklogBytes)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(maxBacklog);
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(maxBacklogBytes);
        _maxBacklogBytes = maxBacklogBytes;
        _messages = Channel.CreateBounded<byte[]>(
            new BoundedChannelOptions(maxBacklog)
            {
                SingleReader = true,
                SingleWriter = false,
            });
    }

    public CloseReason? CompletionReason =>
        Volatile.Read(ref _completed) == 0 ? null : _completionReason;
    public CancellationToken CompletionToken => _completionCts.Token;

    public ChannelByteSendQueueWriteOutcome TryEnqueue(byte[] message)
    {
        lock (_completionLock)
        {
            if (_completed != 0)
            {
                return ChannelByteSendQueueWriteOutcome.Closed;
            }
            if (message.Length > _maxBacklogBytes - _queuedBytes)
            {
                return ChannelByteSendQueueWriteOutcome.Full;
            }

            if (!_messages.Writer.TryWrite(message))
            {
                return _completed != 0
                    ? ChannelByteSendQueueWriteOutcome.Closed
                    : ChannelByteSendQueueWriteOutcome.Full;
            }
            _queuedBytes += message.Length;
            return ChannelByteSendQueueWriteOutcome.Enqueued;
        }
    }

    public async Task<byte[]?> ReadAsync(CancellationToken ct)
    {
        while (await _messages.Reader.WaitToReadAsync(ct))
        {
            lock (_completionLock)
            {
                if (_discardPending != 0)
                {
                    while (_messages.Reader.TryRead(out _))
                    {
                    }
                    _queuedBytes = 0;
                    return null;
                }

                if (_messages.Reader.TryRead(out var message))
                {
                    _queuedBytes -= message.Length;
                    return message;
                }
            }
        }

        return null;
    }

    public void Complete(
        CloseReason completionReason = CloseReason.ClosingNormal,
        bool discardPending = false)
    {
        lock (_completionLock)
        {
            if (_completed != 0)
            {
                return;
            }

            _completionReason = completionReason;
            if (discardPending)
            {
                Volatile.Write(ref _discardPending, 1);
                _queuedBytes = 0;
            }
            Volatile.Write(ref _completed, 1);
            _messages.Writer.TryComplete();
            _completionCts.Cancel();
        }
    }
}
