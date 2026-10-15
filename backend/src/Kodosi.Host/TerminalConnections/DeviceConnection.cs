using System.Net.WebSockets;
using System.Threading.Channels;

namespace Kodosi.TerminalConnections;

/// <summary>
/// The WebSocket of one device. One writer task sends the queue in order. The queue is bounded by bytes: a full relay
/// queue refuses the frame, so that the directory closes only that pipe; a full control queue ends the connection.
/// </summary>
internal sealed class DeviceConnection : IAsyncDisposable
{
    internal const long RelayQueueBytes = 16 * 1024 * 1024;
    private const long QueueBytes = 2 * RelayQueueBytes;
    private const WebSocketCloseStatus ServiceRestart = (WebSocketCloseStatus)1012;
    private static readonly TimeSpan CloseWait = TimeSpan.FromSeconds(2);
    private readonly Channel<Outgoing> queue = Channel.CreateUnbounded<Outgoing>(new UnboundedChannelOptions { SingleReader = true });
    private readonly CancellationTokenSource stopped = new();
    private readonly CancellationTokenSource expired;
    private readonly Lock expirySync = new();
    private readonly RelayBudget relays;
    private readonly TimeProvider clock;
    private readonly Task writer;
    private DateTimeOffset expires;
    private long queued;
    private long heard;
    private long sendingSince;
    private int closing;

    public DeviceConnection(WebSocket socket, Guid userId, string deviceId, RelayBudget relays, TimeProvider clock)
    {
        Socket = socket; UserId = userId; DeviceId = deviceId;
        this.relays = relays;
        this.clock = clock;
        expired = new CancellationTokenSource(Timeout.InfiniteTimeSpan, clock);
        heard = clock.GetTimestamp();
        writer = WriteAsync();
    }

    public WebSocket Socket { get; }
    public Guid UserId { get; }
    public string DeviceId { get; }
    public CancellationToken Stopped => stopped.Token;
    public CancellationToken Expired => expired.Token;
    public bool IsOpen => Volatile.Read(ref closing) == 0 && !stopped.IsCancellationRequested && Socket.State == WebSocketState.Open;

    /// <summary>The time since the device sent its last message.</summary>
    public TimeSpan Silence => clock.GetElapsedTime(Volatile.Read(ref heard));

    /// <summary>The time that the current send waits for the device; zero when no send waits.</summary>
    public TimeSpan Sending => Volatile.Read(ref sendingSince) is var since and not 0 ? clock.GetElapsedTime(since) : TimeSpan.Zero;

    public void Heard() => Volatile.Write(ref heard, clock.GetTimestamp());

    public void ExtendUntil(DateTimeOffset value)
    {
        lock (expirySync)
        {
            if (stopped.IsCancellationRequested || value <= expires) return;
            expires = value;
            var remaining = value - clock.GetUtcNow();
            expired.CancelAfter(remaining > TimeSpan.Zero ? remaining : TimeSpan.Zero);
        }
    }

    /// <summary>Queues a control message. The same bytes can go to many connections.</summary>
    public bool Send(byte[] message)
    {
        if (!IsOpen) return false;
        if (Interlocked.Add(ref queued, message.Length) <= QueueBytes && queue.Writer.TryWrite(new Outgoing(message, WebSocketMessageType.Text))) return true;
        Interlocked.Add(ref queued, -message.Length);
        Abort();
        return false;
    }

    /// <summary>Queues a pipe frame; the connection owns the frame when the result is <see cref="Relayed.Sent"/>.</summary>
    public Relayed Relay(byte[] frame)
    {
        if (!IsOpen) return Relayed.Slow;
        if (Interlocked.Add(ref queued, frame.Length) > RelayQueueBytes)
        {
            Interlocked.Add(ref queued, -frame.Length);
            return Relayed.Slow;
        }
        if (!relays.TryTake(frame.Length))
        {
            Interlocked.Add(ref queued, -frame.Length);
            return Relayed.Busy;
        }
        var outgoing = new Outgoing(frame, WebSocketMessageType.Binary);
        if (queue.Writer.TryWrite(outgoing)) return Relayed.Sent;
        Release(outgoing);
        return Relayed.Slow;
    }

    /// <summary>Closes the connection with "service restart" after the current send; queued messages are dropped.</summary>
    public void Close()
    {
        if (Interlocked.Exchange(ref closing, 1) == 0) queue.Writer.TryComplete();
    }

    public void Abort()
    {
        if (stopped.IsCancellationRequested) return;
        queue.Writer.TryComplete();
        stopped.Cancel();
        Socket.Abort();
    }

    private void Release(Outgoing item)
    {
        Interlocked.Add(ref queued, -item.Bytes.Length);
        if (item.Type == WebSocketMessageType.Binary) relays.Give(item.Bytes.Length);
    }

    private async Task WriteAsync()
    {
        try
        {
            while (await queue.Reader.WaitToReadAsync(Stopped))
                while (queue.Reader.TryRead(out var item))
                {
                    try
                    {
                        if (Volatile.Read(ref closing) != 0) continue;
                        Volatile.Write(ref sendingSince, clock.GetTimestamp());
                        await Socket.SendAsync(item.Bytes, item.Type, true, Stopped);
                    }
                    finally
                    {
                        Volatile.Write(ref sendingSince, 0);
                        Release(item);
                    }
                }
            if (Volatile.Read(ref closing) != 0)
            {
                using var deadline = new CancellationTokenSource(CloseWait, clock);
                using var ending = CancellationTokenSource.CreateLinkedTokenSource(deadline.Token, Stopped);
                await Socket.CloseOutputAsync(ServiceRestart, "The Kodosi server is restarting.", ending.Token);
            }
        }
        catch (Exception error) when (error is OperationCanceledException or WebSocketException or IOException or ObjectDisposedException)
        {
        }
        finally
        {
            Abort();
            while (queue.Reader.TryRead(out var left)) Release(left);
        }
    }

    public async ValueTask DisposeAsync()
    {
        Abort();
        await writer;
        lock (expirySync) expired.Dispose();
        stopped.Dispose();
        Socket.Dispose();
    }

    private readonly record struct Outgoing(byte[] Bytes, WebSocketMessageType Type);
}

internal sealed record Pipe(Guid Id, Guid SessionId, DeviceConnection Viewer, DeviceConnection Host);

internal enum Relayed { Sent, Slow, Busy }

/// <summary>The bytes that wait in all relay queues of the server together.</summary>
internal sealed class RelayBudget(long limit)
{
    public const long ServerBytes = 256L * 1024 * 1024;
    private long used;

    public long Used => Interlocked.Read(ref used);

    public bool TryTake(long bytes)
    {
        if (Interlocked.Add(ref used, bytes) <= limit) return true;
        Interlocked.Add(ref used, -bytes);
        return false;
    }

    public void Give(long bytes) => Interlocked.Add(ref used, -bytes);
}
