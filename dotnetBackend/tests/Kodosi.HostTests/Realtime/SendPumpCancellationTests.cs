using System.Net.WebSockets;
using Microsoft.Extensions.Logging.Abstractions;
using Kodosi.Domain;
using Kodosi.Host.Auth;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class SendPumpCancellationTests
{
    [Fact]
    public async Task HostQueueCompletion_Cancels_Blocked_Send()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queue = new ChannelByteSendQueue();
        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueue([1]));
        using var webSocket = new BlockingSendWebSocket();
        var pump = new HostSessionPump(
            broadcaster,
            null!,
            null!,
            metrics,
            NullLogger<HostSessionPump>.Instance);
        var sending = pump.SendPumpAsync(
            webSocket,
            queue,
            Kodosi.Domain.SessionId.New(),
            queue.CompletionToken);
        await webSocket.SendStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        queue.Complete(CloseReason.HostStopped);

        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => sending.WaitAsync(
                TimeSpan.FromSeconds(1),
                TestContext.Current.CancellationToken));
        Assert.True(webSocket.SendCancellationObserved);
    }

    [Fact]
    public async Task ParticipantQueueCompletion_Bounds_Blocked_Send_Before_Abort()
    {
        var queue = new RelayClientSendQueue();
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueFrame(WireMessage.EncryptedBinary([1])));
        using var lifetime = new CancellationTokenSource();
        using var webSocket = new BlockingSendWebSocket(
            ignoreCancellationUntilAbort: true);
        var sending = ParticipantSessionPump.SendPumpAsync(
            webSocket,
            queue,
            lifetime.Token,
            queue.CompletionToken,
            TimeSpan.FromMilliseconds(25));
        await webSocket.SendStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        queue.Complete(
            QueueCompletionCause.AccessRevokedCascade,
            discardPending: true,
            closeReason: CloseReason.AccessRevoked);

        await sending.WaitAsync(
            TimeSpan.FromSeconds(1),
            TestContext.Current.CancellationToken);
        Assert.True(webSocket.AbortCalled);
    }

    [Fact]
    public async Task HostOrdinarySend_Aborts_When_Peer_Ignores_Cancellation()
    {
        var runtimes = new LiveSessionStateDirectory();
        var metrics = new OperationalMetrics(runtimes);
        var broadcaster = new SessionBroadcaster(
            runtimes,
            metrics,
            NullLoggerFactory.Instance);
        var queue = new ChannelByteSendQueue();
        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueue([1]));
        using var webSocket = new BlockingSendWebSocket(
            ignoreCancellationUntilAbort: true);
        var pump = new HostSessionPump(
            broadcaster,
            null!,
            null!,
            metrics,
            NullLogger<HostSessionPump>.Instance);

        await pump.SendPumpAsync(
            webSocket,
            queue,
            Kodosi.Domain.SessionId.New(),
            TestContext.Current.CancellationToken,
            TimeSpan.FromMilliseconds(25));

        Assert.True(webSocket.AbortCalled);
    }

    [Fact]
    public async Task ParticipantOrdinarySend_Aborts_When_Peer_Ignores_Cancellation()
    {
        var queue = new RelayClientSendQueue();
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueFrame(WireMessage.EncryptedBinary([1])));
        using var lifetime = new CancellationTokenSource();
        using var webSocket = new BlockingSendWebSocket(
            ignoreCancellationUntilAbort: true);

        await ParticipantSessionPump.SendPumpAsync(
            webSocket,
            queue,
            lifetime.Token,
            queue.CompletionToken,
            TimeSpan.FromMilliseconds(25));

        Assert.True(webSocket.AbortCalled);
    }

    [Fact]
    public async Task ParticipantCompletion_Finishes_Bounded_Active_Send_Then_Sends_Terminal()
    {
        var queue = new RelayClientSendQueue();
        var protectedData = WireMessage.EncryptedBinary([1, 2, 3]);
        var terminal = WireMessage.Json([9, 9]);
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueFrame(protectedData));
        using var lifetime = new CancellationTokenSource();
        using var webSocket = new BlockingSendWebSocket(
            completeWhenReleased: true);
        var sending = ParticipantSessionPump.SendPumpAsync(
            webSocket,
            queue,
            lifetime.Token,
            queue.CompletionToken);
        await webSocket.SendStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        Assert.True(queue.TryCompleteWithControlMessage(
            terminal,
            QueueCompletionCause.SessionEnd,
            CloseReason.SessionEnded));
        await Task.Delay(75, TestContext.Current.CancellationToken);
        Assert.False(
            sending.IsCompleted,
            "queue completion must not cancel and invalidate the active WebSocket send");
        Assert.False(webSocket.SendCancellationObserved);
        webSocket.ReleaseSend();
        await sending.WaitAsync(
            TimeSpan.FromSeconds(1),
            TestContext.Current.CancellationToken);

        Assert.Equal(2, webSocket.Attempts.Count);
        Assert.Equal(protectedData.Payload, webSocket.Attempts[0]);
        Assert.Equal(terminal.Payload, webSocket.Attempts[1]);
        Assert.False(webSocket.SendCancellationObserved);
    }

    [Fact]
    public async Task ManagedWebSocket_Completes_Active_Send_Before_Terminal_Without_Abort()
    {
        var queue = new RelayClientSendQueue();
        var protectedData = WireMessage.EncryptedBinary([1, 2, 3, 4]);
        var terminal = WireMessage.Json([9, 8, 7]);
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueFrame(protectedData));
        using var lifetime = new CancellationTokenSource();
        using var stream = new GatedWriteStream();
        using var webSocket = WebSocket.CreateFromStream(
            stream,
            isServer: true,
            subProtocol: null,
            keepAliveInterval: Timeout.InfiniteTimeSpan);
        var sending = ParticipantSessionPump.SendPumpAsync(
            webSocket,
            queue,
            lifetime.Token,
            queue.CompletionToken,
            TimeSpan.FromSeconds(1));
        await stream.FirstWriteStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        Assert.True(queue.TryCompleteWithControlMessage(
            terminal,
            QueueCompletionCause.SessionEnd,
            CloseReason.SessionEnded));
        await Task.Delay(75, TestContext.Current.CancellationToken);
        Assert.Equal(WebSocketState.Open, webSocket.State);
        Assert.False(sending.IsCompleted);

        stream.ReleaseWrites();
        await sending.WaitAsync(
            TimeSpan.FromSeconds(1),
            TestContext.Current.CancellationToken);

        var written = stream.WrittenBytes;
        Assert.True(ContainsSequence(written, protectedData.Payload));
        Assert.True(ContainsSequence(written, terminal.Payload));
        Assert.True(
            IndexOfSequence(written, protectedData.Payload)
                < IndexOfSequence(written, terminal.Payload));
        Assert.Equal(WebSocketState.Open, webSocket.State);
    }

    [Fact]
    public async Task ManagedWebSocket_Ordinary_Send_Aborts_Only_After_Deadline()
    {
        var queue = new RelayClientSendQueue();
        Assert.Equal(
            RelayClientSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueueFrame(WireMessage.EncryptedBinary([1, 2, 3])));
        using var lifetime = new CancellationTokenSource();
        using var stream = new GatedWriteStream();
        using var webSocket = WebSocket.CreateFromStream(
            stream,
            isServer: true,
            subProtocol: null,
            keepAliveInterval: Timeout.InfiniteTimeSpan);
        var startedAt = DateTimeOffset.UtcNow;

        await ParticipantSessionPump.SendPumpAsync(
            webSocket,
            queue,
            lifetime.Token,
            queue.CompletionToken,
            TimeSpan.FromMilliseconds(75));

        Assert.True(DateTimeOffset.UtcNow - startedAt >= TimeSpan.FromMilliseconds(50));
        Assert.Equal(WebSocketState.Aborted, webSocket.State);
    }

    [Fact]
    public async Task ParticipantCompletion_Sends_Terminal_Already_Dequeued_By_Send_Pump()
    {
        var queue = new RelayClientSendQueue();
        var terminal = WireMessage.Json([7, 8, 9]);
        Assert.True(queue.TryCompleteWithControlMessage(
            terminal,
            QueueCompletionCause.SessionEnd,
            CloseReason.SessionEnded));
        using var lifetime = new CancellationTokenSource();
        using var webSocket = new BlockingSendWebSocket(
            completeImmediately: true);

        await ParticipantSessionPump.SendPumpAsync(
            webSocket,
            queue,
            lifetime.Token,
            queue.CompletionToken);

        var attempt = Assert.Single(webSocket.Attempts);
        Assert.Equal(terminal.Payload, attempt);
    }

    [Fact]
    public async Task ParticipantTerminal_DequeueBeforeCompletionCancellation_SendsExactlyOnce()
    {
        var queue = new RelayClientSendQueue();
        var terminal = WireMessage.Json([4, 5, 6]);
        Assert.True(queue.TryCompleteWithControlMessage(
            terminal,
            QueueCompletionCause.SessionEnd,
            CloseReason.SessionEnded));
        using var lifetime = new CancellationTokenSource();
        using var delayedCompletion = new CancellationTokenSource();
        using var webSocket = new BlockingSendWebSocket(
            completeWhenReleased: true);
        var sending = ParticipantSessionPump.SendPumpAsync(
            webSocket,
            queue,
            lifetime.Token,
            delayedCompletion.Token,
            TimeSpan.FromSeconds(1));
        await webSocket.SendStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        await delayedCompletion.CancelAsync();
        Assert.False(webSocket.SendCancellationObserved);
        Assert.False(sending.IsCompleted);

        webSocket.ReleaseSend();
        await sending.WaitAsync(
            TimeSpan.FromSeconds(1),
            TestContext.Current.CancellationToken);

        var attempt = Assert.Single(webSocket.Attempts);
        Assert.Equal(terminal.Payload, attempt);
        Assert.False(webSocket.SendCancellationObserved);
    }

    [Fact]
    public async Task ParticipantTerminalSend_Aborts_When_Peer_Does_Not_Read()
    {
        var queue = new RelayClientSendQueue();
        var terminal = WireMessage.Json([3, 2, 1]);
        Assert.True(queue.TryCompleteWithControlMessage(
            terminal,
            QueueCompletionCause.SessionEnd,
            CloseReason.SessionEnded));
        using var lifetime = new CancellationTokenSource();
        using var webSocket = new BlockingSendWebSocket(
            ignoreCancellationUntilAbort: true);

        await ParticipantSessionPump.SendPumpAsync(
            webSocket,
            queue,
            lifetime.Token,
            queue.CompletionToken,
            TimeSpan.FromMilliseconds(25));

        var attempt = Assert.Single(webSocket.Attempts);
        Assert.Equal(terminal.Payload, attempt);
        Assert.True(webSocket.AbortCalled);
    }

    [Fact]
    public async Task UserEventQueueCompletion_Cancels_Cooperative_Blocked_Send()
    {
        var queue = new ChannelByteSendQueue();
        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueue([1]));
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(
            TestContext.Current.CancellationToken,
            queue.CompletionToken);
        using var webSocket = new BlockingSendWebSocket();
        var sending = UserEventsWebSocketHandler.SendPumpAsync(
            webSocket,
            queue,
            linked.Token,
            TimeSpan.FromSeconds(1));
        await webSocket.SendStarted.Task.WaitAsync(
            TestContext.Current.CancellationToken);

        queue.Complete(
            CloseReason.AccessRevoked,
            discardPending: true);

        await sending.WaitAsync(
            TimeSpan.FromSeconds(1),
            TestContext.Current.CancellationToken);
        Assert.True(linked.IsCancellationRequested);
        Assert.True(webSocket.SendCancellationObserved);
        Assert.False(webSocket.AbortCalled);
    }

    [Fact]
    public async Task UserEventSend_Is_Bounded_When_Peer_Does_Not_Read()
    {
        var queue = new ChannelByteSendQueue();
        Assert.Equal(
            ChannelByteSendQueueWriteOutcome.Enqueued,
            queue.TryEnqueue([1]));
        using var webSocket = new BlockingSendWebSocket(
            ignoreCancellationUntilAbort: true);

        await UserEventsWebSocketHandler.SendPumpAsync(
            webSocket,
            queue,
            TestContext.Current.CancellationToken,
            TimeSpan.FromMilliseconds(25));

        Assert.True(webSocket.AbortCalled);
    }

    private sealed class BlockingSendWebSocket(
        bool completeAfterCancellation = false,
        bool completeImmediately = false,
        bool ignoreCancellationUntilAbort = false,
        bool completeWhenReleased = false) : WebSocket
    {
        private readonly TaskCompletionSource _aborted =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        private readonly TaskCompletionSource _sendReleased =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource SendStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public bool SendCancellationObserved { get; private set; }
        public bool AbortCalled { get; private set; }
        public List<byte[]> Attempts { get; } = [];
        public override WebSocketCloseStatus? CloseStatus => null;
        public override string? CloseStatusDescription => null;
        public override WebSocketState State => WebSocketState.Open;
        public override string? SubProtocol => null;

        public override void Abort()
        {
            AbortCalled = true;
            _aborted.TrySetResult();
        }
        public void ReleaseSend() => _sendReleased.TrySetResult();
        public override Task CloseAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken) => Task.CompletedTask;
        public override Task CloseOutputAsync(
            WebSocketCloseStatus closeStatus,
            string? statusDescription,
            CancellationToken cancellationToken) => Task.CompletedTask;
        public override void Dispose() { }
        public override Task<WebSocketReceiveResult> ReceiveAsync(
            ArraySegment<byte> buffer,
            CancellationToken cancellationToken) =>
            throw new NotSupportedException();
        public override async Task SendAsync(
            ArraySegment<byte> buffer,
            WebSocketMessageType messageType,
            bool endOfMessage,
            CancellationToken cancellationToken)
        {
            Attempts.Add(buffer.ToArray());
            SendStarted.TrySetResult();
            if (completeImmediately
                || (completeAfterCancellation && SendCancellationObserved))
            {
                return;
            }
            if (ignoreCancellationUntilAbort)
            {
                await _aborted.Task;
                return;
            }
            try
            {
                if (completeWhenReleased)
                {
                    await _sendReleased.Task.WaitAsync(cancellationToken);
                }
                else
                {
                    await Task.Delay(
                        Timeout.InfiniteTimeSpan,
                        cancellationToken);
                }
            }
            catch (OperationCanceledException)
            {
                SendCancellationObserved = true;
                throw;
            }
        }
    }

    private sealed class GatedWriteStream : Stream
    {
        private readonly TaskCompletionSource _releaseWrites =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        private readonly MemoryStream _written = new();
        private readonly Lock _lock = new();

        public TaskCompletionSource FirstWriteStarted { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public byte[] WrittenBytes
        {
            get
            {
                lock (_lock)
                {
                    return _written.ToArray();
                }
            }
        }

        public void ReleaseWrites() => _releaseWrites.TrySetResult();

        public override bool CanRead => true;
        public override bool CanSeek => false;
        public override bool CanWrite => true;
        public override long Length => throw new NotSupportedException();
        public override long Position
        {
            get => throw new NotSupportedException();
            set => throw new NotSupportedException();
        }

        public override void Flush() { }
        public override Task FlushAsync(CancellationToken cancellationToken) =>
            Task.CompletedTask;
        public override int Read(byte[] buffer, int offset, int count) =>
            throw new NotSupportedException();
        public override Task<int> ReadAsync(
            byte[] buffer,
            int offset,
            int count,
            CancellationToken cancellationToken) =>
            Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken)
                .ContinueWith(
                    static _ => 0,
                    CancellationToken.None,
                    TaskContinuationOptions.ExecuteSynchronously,
                    TaskScheduler.Default);
        public override long Seek(long offset, SeekOrigin origin) =>
            throw new NotSupportedException();
        public override void SetLength(long value) =>
            throw new NotSupportedException();
        public override void Write(byte[] buffer, int offset, int count) =>
            throw new NotSupportedException();
        public override Task WriteAsync(
            byte[] buffer,
            int offset,
            int count,
            CancellationToken cancellationToken) =>
            WriteAsync(
                new ReadOnlyMemory<byte>(buffer, offset, count),
                cancellationToken).AsTask();
        public override async ValueTask WriteAsync(
            ReadOnlyMemory<byte> buffer,
            CancellationToken cancellationToken = default)
        {
            FirstWriteStarted.TrySetResult();
            await _releaseWrites.Task.WaitAsync(cancellationToken);
            lock (_lock)
            {
                _written.Write(buffer.Span);
            }
        }

        protected override void Dispose(bool disposing)
        {
            _releaseWrites.TrySetResult();
            if (disposing)
            {
                _written.Dispose();
            }
            base.Dispose(disposing);
        }
    }

    private static bool ContainsSequence(byte[] haystack, byte[] needle) =>
        IndexOfSequence(haystack, needle) >= 0;

    private static int IndexOfSequence(byte[] haystack, byte[] needle)
    {
        for (var index = 0; index <= haystack.Length - needle.Length; index++)
        {
            if (haystack.AsSpan(index, needle.Length).SequenceEqual(needle))
            {
                return index;
            }
        }
        return -1;
    }
}
