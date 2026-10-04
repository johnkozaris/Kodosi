using System.Diagnostics;
using System.Net.WebSockets;
using System.Threading.Channels;

namespace Kodosi.TerminalConnections;

internal sealed record Outbound(WebSocketMessageType Type, byte[] Bytes, Func<bool>? StillValid = null, byte[]? Checkpoint = null, ulong? Output = null)
{
    public int Size => Bytes.Length + (Checkpoint?.Length ?? 0);
}

internal sealed class SocketPeer : IAsyncDisposable
{
    private const int MaxBytes = 20 * 1024 * 1024;
    private const int OutputFrames = 192;
    private const int MinimumWindow = 16 * 1024;
    private const int MinimumBacklog = 64 * 1024;
    private const int MaximumBacklog = 8 * 1024 * 1024;
    private const double TransitSeconds = 0.5;
    private static readonly TimeSpan RateSample = TimeSpan.FromMilliseconds(200);
    private readonly Channel<Outbound> queue = Channel.CreateBounded<Outbound>(new BoundedChannelOptions(256)
    { SingleReader = true, FullMode = BoundedChannelFullMode.Wait });
    private readonly Channel<Outbound> urgent = Channel.CreateBounded<Outbound>(new BoundedChannelOptions(256)
    { SingleReader = true, FullMode = BoundedChannelFullMode.Wait });
    private readonly Channel<bool> wake = Channel.CreateBounded<bool>(new BoundedChannelOptions(1)
    { SingleReader = true, FullMode = BoundedChannelFullMode.DropWrite });
    private readonly object flowSync = new();
    private readonly Queue<(ulong Next, int Size)> transit = new();
    private int transitBytes;
    private int snapshotBytes;
    private int sampleBytes;
    private long sampleStarted;
    private bool pressed;
    private double rate;
    private int backlog;
    private readonly CancellationTokenSource stopped = new();
    private readonly CancellationTokenSource expired = new();
    private readonly object expirySync = new();
    private DateTimeOffset expires;
    private int queuedBytes;
    private readonly object drainSync = new();
    private Task? drain;
    private volatile bool draining;
    private readonly Task writer;

    public SocketPeer(WebSocket socket, Guid userId, string deviceId, string connectionId)
    {
        Socket = socket; UserId = userId; DeviceId = deviceId; ConnectionId = connectionId;
        writer = WriteAsync();
    }
    public WebSocket Socket { get; }
    public Guid UserId { get; }
    public string DeviceId { get; }
    public string ConnectionId { get; }
    public CancellationToken Stopped => stopped.Token;
    public CancellationToken Expired => expired.Token;
    public bool IsOpen => !stopped.IsCancellationRequested && Socket.State == WebSocketState.Open;

    public void ExtendUntil(DateTimeOffset value)
    {
        lock (expirySync)
        {
            if (stopped.IsCancellationRequested || value <= expires) return;
            expires = value;
            var remaining = value - DateTimeOffset.UtcNow;
            expired.CancelAfter(remaining > TimeSpan.Zero ? remaining : TimeSpan.Zero);
        }
    }

    public bool HasOutputRoom(int size)
    {
        var waiting = Volatile.Read(ref backlog);
        if (queue.Reader.Count >= OutputFrames) return false;
        if (waiting == 0) return true;
        lock (flowSync)
            return waiting + size <= Math.Clamp(rate * TransitSeconds, Math.Clamp(snapshotBytes, MinimumBacklog, MaximumBacklog), MaximumBacklog);
    }

    public void Acknowledge(ulong sequence)
    {
        lock (flowSync)
        {
            var bytes = 0;
            while (transit.TryPeek(out var head) && head.Next <= sequence) { transit.Dequeue(); bytes += head.Size; }
            if (bytes == 0) return;
            transitBytes -= bytes;
            if (!pressed) { sampleBytes = 0; sampleStarted = 0; }
            else if (sampleStarted == 0) sampleStarted = Stopwatch.GetTimestamp();
            else
            {
                sampleBytes += bytes;
                var elapsed = Stopwatch.GetElapsedTime(sampleStarted);
                if (elapsed >= RateSample)
                {
                    var sample = sampleBytes / elapsed.TotalSeconds;
                    rate = rate == 0 ? sample : (rate + sample) / 2;
                    sampleBytes = 0; sampleStarted = Stopwatch.GetTimestamp(); pressed = false;
                }
            }
        }
        wake.Writer.TryWrite(true);
    }

    public bool Send(object value, Func<bool>? stillValid = null)
        => Send(new Outbound(WebSocketMessageType.Text, Wire.Encode(value), stillValid));
    public bool SendUrgent(object value, Func<bool>? stillValid = null)
        => Send(new Outbound(WebSocketMessageType.Text, Wire.Encode(value), stillValid), urgent);
    public bool SendBinary(byte[] value, Func<bool>? stillValid = null)
        => Send(new Outbound(WebSocketMessageType.Binary, value, stillValid));
    public bool SendOutput(byte[] value, ulong next, Func<bool> stillValid)
        => Send(new Outbound(WebSocketMessageType.Binary, value, stillValid, Output: next));
    public bool SendCheckpoint(object proof, byte[] frame, Func<bool> stillValid)
        => Send(new Outbound(WebSocketMessageType.Text, Wire.Encode(proof), stillValid, frame));
    private bool Send(Outbound frame, Channel<Outbound>? target = null)
    {
        lock (drainSync)
        {
            if (draining) return false;
            return Enqueue(frame, target ?? queue);
        }
    }
    private bool Enqueue(Outbound frame, Channel<Outbound> target)
    {
        if (!IsOpen) return false;
        var bytes = Interlocked.Add(ref queuedBytes, frame.Size);
        if (bytes <= MaxBytes && target.Writer.TryWrite(frame))
        {
            if (frame.Output is not null) Interlocked.Add(ref backlog, frame.Size);
            wake.Writer.TryWrite(true); return true;
        }
        Interlocked.Add(ref queuedBytes, -frame.Size);
        Abort(); return false;
    }
    public Task DrainAndCloseAsync(object final)
    {
        lock (drainSync)
        {
            if (drain is not null) return drain;
            draining = true;
            Enqueue(new Outbound(WebSocketMessageType.Text, Wire.Encode(final)), queue);
            queue.Writer.TryComplete(); wake.Writer.TryWrite(true);
            return drain = DrainAsync();
        }
    }
    private async Task DrainAsync()
    {
        try { await writer.WaitAsync(TimeSpan.FromSeconds(10)); }
        catch (TimeoutException) { Abort(); return; }
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(1));
        try { await Socket.CloseOutputAsync(WebSocketCloseStatus.NormalClosure, "Terminal ended", deadline.Token); }
        catch (Exception error) when (error is WebSocketException or OperationCanceledException or ObjectDisposedException) { }
    }

    public void Abort()
    {
        if (stopped.IsCancellationRequested) return;
        stopped.Cancel(); queue.Writer.TryComplete(); urgent.Writer.TryComplete(); Socket.Abort();
    }
    private bool Holds(Outbound frame)
    {
        if (frame.Output is not { } next || draining) return false;
        lock (flowSync)
        {
            if (transitBytes >= Math.Clamp(rate * TransitSeconds, MinimumWindow, MaximumBacklog)) { pressed = true; return true; }
            transit.Enqueue((next, frame.Size)); transitBytes += frame.Size;
            return false;
        }
    }
    private async Task WriteAsync()
    {
        try
        {
            Outbound? held = null;
            while (true)
            {
                bool stale;
                if (urgent.Reader.TryRead(out var frame)) stale = frame.StillValid?.Invoke() == false;
                else
                {
                    if (held is null && !queue.Reader.TryRead(out held))
                    {
                        if (queue.Reader.Completion.IsCompleted) return;
                        lock (flowSync) { pressed = false; sampleBytes = 0; sampleStarted = 0; }
                        await wake.Reader.ReadAsync(stopped.Token); continue;
                    }
                    stale = held.StillValid?.Invoke() == false;
                    if (!stale && Holds(held)) { await wake.Reader.ReadAsync(stopped.Token); continue; }
                    frame = held; held = null;
                    if (frame.Output is not null) Interlocked.Add(ref backlog, -frame.Size);
                }
                Interlocked.Add(ref queuedBytes, -frame.Size);
                if (stale) continue;
                if (frame.Checkpoint is not null) lock (flowSync) snapshotBytes = frame.Size;
                using var deadline = CancellationTokenSource.CreateLinkedTokenSource(stopped.Token);
                deadline.CancelAfter(TimeSpan.FromSeconds(10));
                await Socket.SendAsync(frame.Bytes, frame.Type, true, deadline.Token);
                if (frame.Checkpoint is { } checkpoint)
                {
                    if (frame.StillValid?.Invoke() == false) continue;
                    await Socket.SendAsync(checkpoint, WebSocketMessageType.Binary, true, deadline.Token);
                }
            }
        }
        catch (Exception error) when (error is OperationCanceledException or WebSocketException or ObjectDisposedException)
        { Abort(); }
    }
    public async ValueTask DisposeAsync()
    {
        Abort(); await writer;
        lock (expirySync) expired.Dispose();
        stopped.Dispose(); Socket.Dispose();
    }
}

internal static class Wire
{
    public static readonly System.Text.Json.JsonSerializerOptions Json = new(System.Text.Json.JsonSerializerDefaults.Web)
    { UnmappedMemberHandling = System.Text.Json.Serialization.JsonUnmappedMemberHandling.Disallow, MaxDepth = 32 };
    public static byte[] Encode(object value) => System.Text.Json.JsonSerializer.SerializeToUtf8Bytes(value, Json);

    public static async Task<(WebSocketMessageType Type, byte[] Bytes)?> ReadAsync(WebSocket socket, int maximum, CancellationToken ct)
    {
        using var buffer = new MemoryStream();
        var chunk = new byte[16 * 1024];
        WebSocketMessageType? type = null;
        while (true)
        {
            var read = await socket.ReceiveAsync(chunk.AsMemory(), ct);
            if (read.MessageType == WebSocketMessageType.Close) return null;
            if (type is not null && type != read.MessageType) throw ApiException.Invalid("Mixed WebSocket fragment types.");
            type = read.MessageType;
            if (buffer.Length + read.Count > maximum) throw ApiException.Invalid("WebSocket frame is too large.");
            buffer.Write(chunk, 0, read.Count);
            if (read.EndOfMessage) return (type.Value, buffer.ToArray());
        }
    }
    public static System.Text.Json.JsonDocument Parse(byte[] bytes) => System.Text.Json.JsonDocument.Parse(bytes,
        new System.Text.Json.JsonDocumentOptions { MaxDepth = 32 });
}
