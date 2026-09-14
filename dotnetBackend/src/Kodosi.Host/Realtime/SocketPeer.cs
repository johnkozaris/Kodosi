using System.Net.WebSockets;
using System.Threading.Channels;

namespace Kodosi.Realtime;

internal sealed record Outbound(WebSocketMessageType Type, byte[] Bytes, Func<bool>? StillValid = null, byte[]? Checkpoint = null)
{
    public int Size => Bytes.Length + (Checkpoint?.Length ?? 0);
}

internal sealed class SocketPeer : IAsyncDisposable
{
    private const int MaxBytes = 20 * 1024 * 1024;
    private readonly Channel<Outbound> queue = Channel.CreateBounded<Outbound>(new BoundedChannelOptions(256)
    { SingleReader = true, FullMode = BoundedChannelFullMode.Wait });
    private readonly CancellationTokenSource stopped = new();
    private int queuedBytes;
    private readonly object drainSync = new();
    private Task? drain;
    private bool draining;
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
    public bool IsOpen => !stopped.IsCancellationRequested && Socket.State == WebSocketState.Open;

    public bool Send(object value, Func<bool>? stillValid = null)
        => Send(new Outbound(WebSocketMessageType.Text, Wire.Encode(value), stillValid));
    public bool SendBinary(byte[] value, Func<bool>? stillValid = null)
        => Send(new Outbound(WebSocketMessageType.Binary, value, stillValid));
    public bool SendCheckpoint(object proof, byte[] frame, Func<bool> stillValid)
        => Send(new Outbound(WebSocketMessageType.Text, Wire.Encode(proof), stillValid, frame));
    private bool Send(Outbound frame)
    {
        lock (drainSync)
        {
            if (draining) return false;
            return Enqueue(frame);
        }
    }
    private bool Enqueue(Outbound frame)
    {
        if (!IsOpen) return false;
        var bytes = Interlocked.Add(ref queuedBytes, frame.Size);
        if (bytes <= MaxBytes && queue.Writer.TryWrite(frame)) return true;
        Interlocked.Add(ref queuedBytes, -frame.Size);
        Abort(); return false;
    }
    public Task DrainAndCloseAsync(object final)
    {
        lock (drainSync)
        {
            if (drain is not null) return drain;
            draining = true;
            Enqueue(new Outbound(WebSocketMessageType.Text, Wire.Encode(final)));
            queue.Writer.TryComplete();
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
        stopped.Cancel(); queue.Writer.TryComplete(); Socket.Abort();
    }
    private async Task WriteAsync()
    {
        try
        {
            await foreach (var frame in queue.Reader.ReadAllAsync(stopped.Token))
            {
                Interlocked.Add(ref queuedBytes, -frame.Size);
                if (frame.StillValid?.Invoke() == false) continue;
                using var deadline = CancellationTokenSource.CreateLinkedTokenSource(stopped.Token);
                deadline.CancelAfter(TimeSpan.FromSeconds(10));
                await Socket.SendAsync(frame.Bytes, frame.Type, true, deadline.Token);
                if (frame.Checkpoint is { } checkpoint)
                {
                    if (frame.StillValid?.Invoke() == false) { Abort(); return; }
                    await Socket.SendAsync(checkpoint, WebSocketMessageType.Binary, true, deadline.Token);
                }
            }
        }
        catch (Exception error) when (error is OperationCanceledException or WebSocketException or ObjectDisposedException)
        { Abort(); }
    }
    public async ValueTask DisposeAsync()
    {
        Abort(); await writer; stopped.Dispose(); Socket.Dispose();
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
