using System.Buffers;
using System.Net.WebSockets;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Kodosi.TerminalConnections;

[JsonPolymorphic(TypeDiscriminatorPropertyName = "type")]
[JsonDerivedType(typeof(PingMessage), "ping")]
[JsonDerivedType(typeof(PongMessage), "pong")]
[JsonDerivedType(typeof(HostMessage), "host")]
[JsonDerivedType(typeof(UnhostMessage), "unhost")]
[JsonDerivedType(typeof(StatusMessage), "status")]
[JsonDerivedType(typeof(OpenMessage), "open")]
[JsonDerivedType(typeof(CloseMessage), "close")]
internal abstract record DeviceMessage;

internal sealed record PingMessage : DeviceMessage;

internal sealed record PongMessage : DeviceMessage;

internal sealed record HostMessage(Guid SessionId, Guid IncarnationId) : DeviceMessage;

internal sealed record UnhostMessage(Guid SessionId) : DeviceMessage;

internal sealed record StatusMessage(Guid SessionId, Guid IncarnationId, TerminalStatus? Status) : DeviceMessage;

internal sealed record OpenMessage(Guid Pipe, Guid SessionId, Guid IncarnationId) : DeviceMessage;

internal sealed record CloseMessage(Guid Pipe) : DeviceMessage;

[JsonPolymorphic(TypeDiscriminatorPropertyName = "type")]
[JsonDerivedType(typeof(ReadyNotice), "ready")]
[JsonDerivedType(typeof(RefusedNotice), "refused")]
[JsonDerivedType(typeof(PongNotice), "pong")]
[JsonDerivedType(typeof(HostedNotice), "hosted")]
[JsonDerivedType(typeof(UnhostedNotice), "unhosted")]
[JsonDerivedType(typeof(ChangedNotice), "changed")]
[JsonDerivedType(typeof(TerminalNotice), "terminal")]
[JsonDerivedType(typeof(ViewerNotice), "viewer")]
[JsonDerivedType(typeof(AccessChangedNotice), "accessChanged")]
[JsonDerivedType(typeof(ClosedNotice), "closed")]
internal abstract record ServerMessage;

internal sealed record ReadyNotice : ServerMessage;

internal sealed record RefusedNotice(
    string Reason,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Oldest = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] int? Current = null) : ServerMessage;

internal sealed record PongNotice : ServerMessage;

internal sealed record HostedNotice(Guid SessionId) : ServerMessage;

internal sealed record UnhostedNotice(Guid SessionId, int Status, string Message) : ServerMessage;

internal sealed record ChangedNotice(
    string Surface,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] Guid? RoomId = null,
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] long? Sequence = null) : ServerMessage;

internal sealed record TerminalNotice(Guid SessionId, Guid IncarnationId, bool Online, TerminalStatus? Status) : ServerMessage;

internal sealed record ViewerNotice(Guid Pipe, Guid SessionId, Guid UserId, string DeviceId) : ServerMessage;

internal sealed record AccessChangedNotice(Guid SessionId) : ServerMessage;

internal sealed record ClosedNotice(Guid Pipe, int Status, string Message) : ServerMessage;

[JsonSourceGenerationOptions(JsonSerializerDefaults.Web, AllowOutOfOrderMetadataProperties = true, RespectNullableAnnotations = true,
    RespectRequiredConstructorParameters = true, MaxDepth = 32)]
[JsonSerializable(typeof(DeviceMessage))]
[JsonSerializable(typeof(ServerMessage))]
internal sealed partial class WireJson : JsonSerializerContext;

internal static class Wire
{
    public static readonly byte[] Ready = Encode(new ReadyNotice());
    public static readonly byte[] Pong = Encode(new PongNotice());

    public static byte[] Encode(ServerMessage message) => JsonSerializer.SerializeToUtf8Bytes(message, WireJson.Default.ServerMessage);

    public static DeviceMessage Decode(ReadOnlySpan<byte> message) =>
        JsonSerializer.Deserialize(message, WireJson.Default.DeviceMessage) ?? throw ApiException.Invalid("Unknown connection message.");
}

internal sealed class MessageReader(WebSocket socket) : IDisposable
{
    private const int Usual = 16 * 1024;
    private byte[] buffer = ArrayPool<byte>.Shared.Rent(Usual);

    public int Length { get; private set; }

    public ReadOnlySpan<byte> Message => buffer.AsSpan(0, Length);

    public ReadOnlyMemory<byte> Memory => buffer.AsMemory(0, Length);

    public async ValueTask<WebSocketMessageType?> ReadAsync(int maximum, CancellationToken ct)
    {
        if (buffer.Length > Usual) Replace(ArrayPool<byte>.Shared.Rent(Usual), 0);
        Length = 0;
        WebSocketMessageType? type = null;
        while (true)
        {
            if (Length == buffer.Length) Replace(ArrayPool<byte>.Shared.Rent(Math.Min(buffer.Length * 2, maximum + 1)), Length);
            var read = await socket.ReceiveAsync(buffer.AsMemory(Length), ct);
            if (read.MessageType == WebSocketMessageType.Close) return null;
            if (type is not null && type != read.MessageType) throw ApiException.Invalid("Mixed WebSocket fragment types.");
            type = read.MessageType;
            Length += read.Count;
            if (Length > maximum) throw ApiException.Invalid("WebSocket frame is too large.");
            if (read.EndOfMessage) return type;
        }
    }

    public void Dispose()
    {
        ArrayPool<byte>.Shared.Return(buffer);
        buffer = [];
    }

    private void Replace(byte[] next, int keep)
    {
        buffer.AsSpan(0, keep).CopyTo(next);
        ArrayPool<byte>.Shared.Return(buffer);
        buffer = next;
    }
}
