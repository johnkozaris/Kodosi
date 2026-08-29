using System.Net.WebSockets;
using Kodosi.Host.Realtime;

namespace Kodosi.HostTests;

public sealed class WebSocketMessageReaderTests
{
    [Fact]
    public async Task ReceiveAsync_Reassembles_Text_Fragments()
    {
        using var webSocket = new ScriptedWebSocket([
            Frame.Text("{\"type\":\"participant."),
            Frame.Text("join\"}", endOfMessage: true),
        ]);

        var result = await WebSocketMessageReader.ReceiveAsync(webSocket, 64, CancellationToken.None);

        Assert.Equal(WebSocketMessageReader.ReadStatus.Message, result.Status);
        Assert.Equal(WebSocketMessageType.Text, result.MessageType);
        Assert.Equal("{\"type\":\"participant.join\"}", System.Text.Encoding.UTF8.GetString(result.Payload));
    }

    [Fact]
    public async Task ReceiveAsync_Reassembles_Binary_Fragments()
    {
        using var webSocket = new ScriptedWebSocket([
            Frame.Binary([1, 2]),
            Frame.Binary([3, 4], endOfMessage: true),
        ]);

        var result = await WebSocketMessageReader.ReceiveAsync(webSocket, 64, CancellationToken.None);

        Assert.Equal(WebSocketMessageReader.ReadStatus.Message, result.Status);
        Assert.Equal(WebSocketMessageType.Binary, result.MessageType);
        Assert.Equal([1, 2, 3, 4], result.Payload);
    }

    [Fact]
    public async Task ReceiveAsync_Returns_TooLarge_When_Fragmented_Message_Exceeds_Limit()
    {
        using var webSocket = new ScriptedWebSocket([
            Frame.Text("1234"),
            Frame.Text("5", endOfMessage: true),
        ]);

        var result = await WebSocketMessageReader.ReceiveAsync(webSocket, 4, CancellationToken.None);

        Assert.Equal(WebSocketMessageReader.ReadStatus.TooLarge, result.Status);
    }

    [Fact]
    public async Task ReceiveAsync_Returns_Closed_When_Close_Frame_Arrives()
    {
        using var webSocket = new ScriptedWebSocket([
            Frame.Close(),
        ]);

        var result = await WebSocketMessageReader.ReceiveAsync(webSocket, 64, CancellationToken.None);

        Assert.Equal(WebSocketMessageReader.ReadStatus.Closed, result.Status);
        Assert.Equal(WebSocketMessageType.Close, result.MessageType);
        Assert.Empty(result.Payload);
    }

    [Fact]
    public async Task HandshakeReader_Times_Out_An_Idle_Peer()
    {
        using var webSocket = new NeverCompletingWebSocket();

        var result = await WebSocketHandshakeReader.ReceiveAsync(
            webSocket,
            64,
            TimeSpan.FromMilliseconds(20),
            TestContext.Current.CancellationToken);

        Assert.Null(result);
    }

    [Fact]
    public async Task CloseOutputAsync_Aborts_Peer_That_Does_Not_Acknowledge_Deadline()
    {
        using var webSocket = new NeverCompletingCloseWebSocket();

        await WebSocketCloseHelper.CloseOutputAsync(
            webSocket,
            WebSocketCloseStatus.NormalClosure,
            "closing_normal",
            TestContext.Current.CancellationToken,
            TimeSpan.FromMilliseconds(20));

        Assert.Equal(WebSocketState.Aborted, webSocket.State);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task CloseOutputAsync_Aborts_When_Close_Throws(
        bool throwSynchronously)
    {
        using var webSocket = new ThrowingCloseWebSocket(throwSynchronously);

        await WebSocketCloseHelper.CloseOutputAsync(
            webSocket,
            WebSocketCloseStatus.NormalClosure,
            "closing_normal",
            TestContext.Current.CancellationToken);

        Assert.Equal(WebSocketState.Aborted, webSocket.State);
    }

    private sealed record Frame(WebSocketMessageType MessageType, byte[] Payload, bool EndOfMessage)
    {
        public static Frame Text(string payload, bool endOfMessage = false)
            => new(WebSocketMessageType.Text, System.Text.Encoding.UTF8.GetBytes(payload), endOfMessage);

        public static Frame Binary(byte[] payload, bool endOfMessage = false)
            => new(WebSocketMessageType.Binary, payload, endOfMessage);

        public static Frame Close()
            => new(WebSocketMessageType.Close, [], true);
    }

    private sealed class ScriptedWebSocket(IReadOnlyList<Frame> frames) : WebSocket
    {
        private readonly Queue<Frame> _frames = new(frames);
        private WebSocketState _state = WebSocketState.Open;

        public override WebSocketCloseStatus? CloseStatus => null;
        public override string? CloseStatusDescription => null;
        public override WebSocketState State => _state;
        public override string? SubProtocol => null;

        public override void Abort()
        {
            _state = WebSocketState.Aborted;
        }

        public override Task CloseAsync(WebSocketCloseStatus closeStatus, string? statusDescription, CancellationToken cancellationToken)
        {
            _state = WebSocketState.Closed;
            return Task.CompletedTask;
        }

        public override Task CloseOutputAsync(WebSocketCloseStatus closeStatus, string? statusDescription, CancellationToken cancellationToken)
        {
            _state = WebSocketState.CloseSent;
            return Task.CompletedTask;
        }

        public override void Dispose()
        {
            _state = WebSocketState.Closed;
        }

        public override Task<WebSocketReceiveResult> ReceiveAsync(ArraySegment<byte> buffer, CancellationToken cancellationToken)
        {
            if (_frames.Count == 0)
            {
                _state = WebSocketState.CloseReceived;
                return Task.FromResult(new WebSocketReceiveResult(0, WebSocketMessageType.Close, true));
            }

            var frame = _frames.Dequeue();
            if (frame.MessageType == WebSocketMessageType.Close)
            {
                _state = WebSocketState.CloseReceived;
                return Task.FromResult(new WebSocketReceiveResult(0, WebSocketMessageType.Close, true));
            }

            if (frame.Payload.Length > buffer.Count)
            {
                throw new InvalidOperationException("Test frame payload exceeds receive buffer.");
            }

            frame.Payload.AsSpan().CopyTo(buffer.AsSpan());
            return Task.FromResult(
                new WebSocketReceiveResult(frame.Payload.Length, frame.MessageType, frame.EndOfMessage));
        }

        public override Task SendAsync(ArraySegment<byte> buffer, WebSocketMessageType messageType, bool endOfMessage, CancellationToken cancellationToken)
            => Task.CompletedTask;
    }

    private sealed class NeverCompletingWebSocket : WebSocket
    {
        public override WebSocketCloseStatus? CloseStatus => null;
        public override string? CloseStatusDescription => null;
        public override WebSocketState State => WebSocketState.Open;
        public override string? SubProtocol => null;
        public override void Abort() { }
        public override Task CloseAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken) => Task.CompletedTask;
        public override Task CloseOutputAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken) => Task.CompletedTask;
        public override void Dispose() { }
        public override async Task<WebSocketReceiveResult> ReceiveAsync(
            ArraySegment<byte> buffer,
            CancellationToken cancellationToken)
        {
            await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
            throw new InvalidOperationException("Infinite delay unexpectedly completed.");
        }
        public override Task SendAsync(
            ArraySegment<byte> buffer,
            WebSocketMessageType messageType,
            bool endOfMessage,
            CancellationToken cancellationToken) => Task.CompletedTask;
    }

    private sealed class ThrowingCloseWebSocket(bool throwSynchronously)
        : WebSocket
    {
        private WebSocketState _state = WebSocketState.Open;

        public override WebSocketCloseStatus? CloseStatus => null;
        public override string? CloseStatusDescription => null;
        public override WebSocketState State => _state;
        public override string? SubProtocol => null;
        public override void Abort() => _state = WebSocketState.Aborted;
        public override Task CloseAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken) => throw new NotSupportedException();
        public override Task CloseOutputAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken)
        {
            if (throwSynchronously)
            {
                throw new WebSocketException("Injected synchronous close failure.");
            }
            return Task.FromException(
                new WebSocketException("Injected asynchronous close failure."));
        }
        public override void Dispose() => _state = WebSocketState.Closed;
        public override Task<WebSocketReceiveResult> ReceiveAsync(
            ArraySegment<byte> buffer,
            CancellationToken cancellationToken) => throw new NotSupportedException();
        public override Task SendAsync(
            ArraySegment<byte> buffer,
            WebSocketMessageType messageType,
            bool endOfMessage,
            CancellationToken cancellationToken) => throw new NotSupportedException();
    }

    private sealed class NeverCompletingCloseWebSocket : WebSocket
    {
        private WebSocketState _state = WebSocketState.Open;

        public override WebSocketCloseStatus? CloseStatus => null;
        public override string? CloseStatusDescription => null;
        public override WebSocketState State => _state;
        public override string? SubProtocol => null;
        public override void Abort() => _state = WebSocketState.Aborted;
        public override Task CloseAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken) => Task.CompletedTask;
        public override async Task CloseOutputAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken)
        {
            await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
        }
        public override void Dispose() => _state = WebSocketState.Closed;
        public override Task<WebSocketReceiveResult> ReceiveAsync(
            ArraySegment<byte> buffer,
            CancellationToken cancellationToken) =>
            throw new NotSupportedException();
        public override Task SendAsync(
            ArraySegment<byte> buffer,
            WebSocketMessageType messageType,
            bool endOfMessage,
            CancellationToken cancellationToken) =>
            throw new NotSupportedException();
    }
}
