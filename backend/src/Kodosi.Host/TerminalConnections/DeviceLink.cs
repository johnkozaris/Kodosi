using System.Net.WebSockets;
using System.Threading.Channels;

namespace Kodosi.TerminalConnections;

internal sealed class DeviceLink : IAsyncDisposable
{
    private const long RelayQueueBytes = 16 * 1024 * 1024;
    private const long QueueBytes = 2 * RelayQueueBytes;
    private readonly Channel<(byte[] Bytes, WebSocketMessageType Type)> queue =
        Channel.CreateUnbounded<(byte[], WebSocketMessageType)>(new UnboundedChannelOptions { SingleReader = true });
    private readonly CancellationTokenSource stopped = new();
    private readonly CancellationTokenSource expired = new();
    private readonly object expirySync = new();
    private readonly Task writer;
    private DateTimeOffset expires;
    private long queued;

    public DeviceLink(WebSocket socket, Guid userId, string deviceId, string connectionId)
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

    public bool Send(object value)
    {
        if (!IsOpen) return false;
        var bytes = Wire.Encode(value);
        if (Interlocked.Add(ref queued, bytes.Length) <= QueueBytes && queue.Writer.TryWrite((bytes, WebSocketMessageType.Text))) return true;
        Abort(); return false;
    }

    public bool Relay(byte[] frame)
    {
        if (!IsOpen) return false;
        if (Interlocked.Add(ref queued, frame.Length) > RelayQueueBytes)
        {
            Interlocked.Add(ref queued, -frame.Length);
            return false;
        }
        return queue.Writer.TryWrite((frame, WebSocketMessageType.Binary));
    }

    public void Abort()
    {
        if (stopped.IsCancellationRequested) return;
        queue.Writer.TryComplete(); stopped.Cancel(); Socket.Abort();
    }

    private async Task WriteAsync()
    {
        try
        {
            await foreach (var (bytes, type) in queue.Reader.ReadAllAsync(Stopped))
            {
                using var deadline = CancellationTokenSource.CreateLinkedTokenSource(Stopped);
                deadline.CancelAfter(TimeSpan.FromSeconds(30));
                await Socket.SendAsync(bytes, type, true, deadline.Token);
                Interlocked.Add(ref queued, -bytes.Length);
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

internal sealed record Pipe(Guid Id, Guid SessionId, DeviceLink Viewer, DeviceLink Host);

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
