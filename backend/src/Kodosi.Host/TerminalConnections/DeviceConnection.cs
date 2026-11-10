using System.Collections.Concurrent;
using System.Net.WebSockets;
using System.Threading.Channels;

namespace Kodosi.TerminalConnections;

internal sealed class DeviceConnection : IAsyncDisposable
{
    internal const long RelayQueueBytes = 16 * 1024 * 1024;
    internal const long BacklogBytes = RelayQueueBytes * 3 / 4;
    internal const int FrameOverhead = 80;
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
    private long backlogSince;
    private int closing;

    public DeviceConnection(WebSocket socket, Guid userId, string deviceId, RelayBudget relays, TimeProvider clock, bool hosts = false)
    {
        Socket = socket; UserId = userId; DeviceId = deviceId; Hosts = hosts;
        this.relays = relays;
        this.clock = clock;
        expired = new CancellationTokenSource(Timeout.InfiniteTimeSpan, clock);
        heard = clock.GetTimestamp();
        writer = WriteAsync();
    }

    public WebSocket Socket { get; }
    public Guid UserId { get; }
    public string DeviceId { get; }
    public bool Hosts { get; }
    public CancellationToken Stopped => stopped.Token;
    public CancellationToken Expired => expired.Token;
    public bool IsOpen => Volatile.Read(ref closing) == 0 && !stopped.IsCancellationRequested && Socket.State == WebSocketState.Open;

    public TimeSpan Silence => clock.GetElapsedTime(Volatile.Read(ref heard));

    public TimeSpan Sending => Volatile.Read(ref sendingSince) is var since and not 0 ? clock.GetElapsedTime(since) : TimeSpan.Zero;

    public TimeSpan Backlog => Volatile.Read(ref backlogSince) is var since and not 0 ? clock.GetElapsedTime(since) : TimeSpan.Zero;

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

    public bool Send(byte[] message)
    {
        if (!IsOpen) return false;
        var cost = Cost(message);
        if (Interlocked.Add(ref queued, cost) <= QueueBytes && queue.Writer.TryWrite(new Outgoing(message, WebSocketMessageType.Text))) return true;
        Interlocked.Add(ref queued, -cost);
        Abort();
        return false;
    }

    public Relayed Relay(byte[] frame)
    {
        if (!IsOpen) return Relayed.Slow;
        var cost = Cost(frame);
        if (Interlocked.Add(ref queued, cost) > RelayQueueBytes)
        {
            Interlocked.Add(ref queued, -cost);
            return Relayed.Slow;
        }
        if (relays.TryTake(UserId, cost) is var taken and not Relayed.Sent)
        {
            Interlocked.Add(ref queued, -cost);
            return taken;
        }
        var outgoing = new Outgoing(frame, WebSocketMessageType.Binary);
        if (queue.Writer.TryWrite(outgoing)) return Relayed.Sent;
        Release(outgoing);
        return Relayed.Slow;
    }

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

    private static long Cost(byte[] bytes) => bytes.Length + FrameOverhead;

    private void Release(Outgoing item)
    {
        var cost = Cost(item.Bytes);
        Interlocked.Add(ref queued, -cost);
        if (item.Type == WebSocketMessageType.Binary) relays.Give(UserId, cost);
    }

    private void NoteBacklog()
    {
        if (Interlocked.Read(ref queued) <= BacklogBytes) Volatile.Write(ref backlogSince, 0);
        else if (Volatile.Read(ref backlogSince) == 0) Volatile.Write(ref backlogSince, clock.GetTimestamp());
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
                        NoteBacklog();
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

internal sealed record Pipe(Guid Id, Guid SessionId, DeviceConnection Viewer, DeviceConnection Host)
{
    public bool IsCall => SessionId == Guid.Empty;
}

internal enum Relayed { Sent, Slow, Busy }

internal sealed class RelayBudget(long limit, long accountLimit = RelayBudget.AccountBytes)
{
    public const long ServerBytes = 256L * 1024 * 1024;
    public const long AccountBytes = 32L * 1024 * 1024;
    private readonly ConcurrentDictionary<Guid, long> accounts = new();
    private long used;

    public long Used => Interlocked.Read(ref used);

    public Relayed TryTake(Guid account, long bytes)
    {
        if (accounts.AddOrUpdate(account, static (_, taken) => taken, static (_, held, taken) => held + taken, bytes) > accountLimit)
        {
            GiveBack(account, bytes);
            return Relayed.Slow;
        }
        if (Interlocked.Add(ref used, bytes) <= limit) return Relayed.Sent;
        Interlocked.Add(ref used, -bytes);
        GiveBack(account, bytes);
        return Relayed.Busy;
    }

    public void Give(Guid account, long bytes)
    {
        Interlocked.Add(ref used, -bytes);
        GiveBack(account, bytes);
    }

    private void GiveBack(Guid account, long bytes)
    {
        if (accounts.AddOrUpdate(account, static (_, given) => -given, static (_, held, given) => held - given, bytes) == 0)
            accounts.TryRemove(KeyValuePair.Create(account, 0L));
    }
}
