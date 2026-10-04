using System.Net.WebSockets;
using System.Threading.Channels;

namespace Kodosi.TerminalConnections;

internal class Peer(WebSocket socket, Guid userId, string deviceId, string connectionId) : IAsyncDisposable
{
    private readonly CancellationTokenSource stopped = new();
    private readonly CancellationTokenSource expired = new();
    private readonly object expirySync = new();
    private DateTimeOffset expires;

    public WebSocket Socket { get; } = socket;
    public Guid UserId { get; } = userId;
    public string DeviceId { get; } = deviceId;
    public string ConnectionId { get; } = connectionId;
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

    public virtual void Abort()
    {
        if (stopped.IsCancellationRequested) return;
        stopped.Cancel(); Socket.Abort();
    }

    public virtual ValueTask DisposeAsync()
    {
        Abort();
        lock (expirySync) expired.Dispose();
        stopped.Dispose(); Socket.Dispose();
        return ValueTask.CompletedTask;
    }
}

internal sealed class SocketPeer : Peer
{
    private readonly Channel<byte[]> queue = Channel.CreateBounded<byte[]>(new BoundedChannelOptions(256)
    { SingleReader = true, FullMode = BoundedChannelFullMode.Wait });
    private readonly Task writer;

    public SocketPeer(WebSocket socket, Guid userId, string deviceId, string connectionId) : base(socket, userId, deviceId, connectionId)
        => writer = WriteAsync();

    public bool Send(object value)
    {
        if (!IsOpen) return false;
        if (queue.Writer.TryWrite(Wire.Encode(value))) return true;
        Abort(); return false;
    }

    public override void Abort()
    {
        queue.Writer.TryComplete(); base.Abort();
    }

    private async Task WriteAsync()
    {
        try
        {
            await foreach (var message in queue.Reader.ReadAllAsync(Stopped))
            {
                using var deadline = CancellationTokenSource.CreateLinkedTokenSource(Stopped);
                deadline.CancelAfter(TimeSpan.FromSeconds(10));
                await Socket.SendAsync(message, WebSocketMessageType.Text, true, deadline.Token);
            }
        }
        catch (Exception error) when (error is OperationCanceledException or WebSocketException or ObjectDisposedException)
        { Abort(); }
    }

    public override async ValueTask DisposeAsync()
    {
        Abort(); await writer;
        await base.DisposeAsync();
    }
}

internal sealed class Pipe(Guid id, Peer viewer)
{
    public Guid Id { get; } = id;
    public Peer Viewer { get; } = viewer;
    public Peer? Host { get; set; }
    public TaskCompletionSource<Peer> Joined { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

    public void Abort()
    {
        Joined.TrySetCanceled(); Viewer.Abort(); Host?.Abort();
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
