using System.Buffers.Binary;
using System.Net.WebSockets;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.Realtime;
using Xunit;

namespace Kodosi.HostTests;

public sealed class RelayTests
{
    internal static byte[] Frame(byte type, int generation, ulong counter, ulong first, ulong next, int size = 64)
    {
        var frame = new byte[size]; frame[0] = type;
        BinaryPrimitives.WriteUInt32BigEndian(frame.AsSpan(1), (uint)generation);
        BinaryPrimitives.WriteUInt64BigEndian(frame.AsSpan(5), counter);
        BinaryPrimitives.WriteUInt64BigEndian(frame.AsSpan(13), first);
        BinaryPrimitives.WriteUInt64BigEndian(frame.AsSpan(21), next); return frame;
    }

    [Fact]
    public void CheckpointAndRawReplayAreBoundedAndRejectRetiredFrameTypes()
    {
        var replay = new TerminalReplay(); replay.Accept(Frame(3, 1, 0, 1, 10), 1);
        replay.Accept(Frame(4, 1, 0, 10, 11), 1);
        var joined = replay.Bootstrap(Frame(3, 1, 1, 2, 10), 1);
        Assert.NotNull(joined); Assert.Equal(2, joined.Count);
        Assert.Throws<ApiException>(() => replay.Accept(Frame(5, 1, 1, 2, 11), 1));
        Assert.Throws<ApiException>(() => replay.Accept(Frame(6, 1, 1, 2, 11), 1));
        Assert.Throws<ApiException>(() => replay.Accept(Frame(4, 1, 0, 11, 12), 1));
        Assert.Throws<ApiException>(() => replay.Accept(Frame(4, 2, 2, 11, 12), 1));
        for (ulong i = 11; i < 300; i++) replay.Accept(Frame(4, 1, i, i, i + 1), 1);
        Assert.Null(replay.Bootstrap(Frame(3, 1, 2, 3, 10), 1));
        Assert.Single(replay.Bootstrap(Frame(3, 1, 3, 4, 300), 1)!);
    }

    [Fact]
    public async Task LateParticipantBootstrapDoesNotResetOtherViewers()
    {
        var directory = new RelayDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        directory.Output(live, host, Frame(3, 1, 0, 1, 10));
        await using var first = Peer(); directory.RegisterParticipant(state, first); directory.BeginBootstrap(live, first, Challenge());
        var request = live.Checkpoints.Keys.Single(); directory.Checkpoint(live, host, request, Frame(3, 1, 1, 2, 10), Signature);
        directory.Output(live, host, Frame(4, 1, 0, 10, 11));
        Assert.Equal(11UL, live.Participants[first.ConnectionId].NextSequence);
        await using var second = Peer(); directory.RegisterParticipant(state, second); directory.BeginBootstrap(live, second, Challenge());
        request = live.Checkpoints.Keys.Single(); directory.Checkpoint(live, host, request, Frame(3, 1, 2, 3, 11), Signature);
        Assert.Equal(11UL, live.Participants[first.ConnectionId].NextSequence);
        Assert.True(live.Participants[first.ConnectionId].Ready); Assert.True(live.Participants[second.ConnectionId].Ready);
        directory.Output(live, host, Frame(4, 1, 1, 11, 12));
        Assert.Equal(12UL, live.Participants[second.ConnectionId].NextSequence);
        directory.RequestCapture(live, first, Challenge());
        Assert.True(live.Participants[first.ConnectionId].Ready); Assert.True(live.Participants[second.ConnectionId].Ready);
        request = live.Checkpoints.Keys.Single();
        directory.Checkpoint(live, host, request, Frame(3, 1, 3, 4, 14), Signature);
        Assert.Equal(12UL, live.Participants[first.ConnectionId].NextSequence);
        directory.Output(live, host, Frame(4, 1, 2, 12, 14));
        Assert.Equal(14UL, live.Participants[first.ConnectionId].NextSequence);
        Assert.Equal(14UL, live.Participants[second.ConnectionId].NextSequence);
    }

    [Fact]
    public async Task RevocationAndHostReplacementFenceConnectionsAndOldControls()
    {
        var directory = new RelayDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        await using var participant = Peer(); directory.RegisterParticipant(state, participant); directory.BeginBootstrap(live, participant, Challenge());
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 0, 1, 0), Signature);
        using var message = JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            type = "control",
            sequence = 1,
            keyGeneration = 1,
            requestId = Guid.CreateVersion7(),
            nonce = Convert.ToBase64String(new byte[12]),
            ciphertext = Convert.ToBase64String(new byte[32]),
            signature = Convert.ToBase64String(new byte[3309])
        }));
        directory.Control(live, participant, message.RootElement);
        Assert.Single(live.Pending);
        Assert.Throws<ApiException>(() => directory.Control(live, participant, message.RootElement));
        state.AuthorizationRevision = 2; state.KeyGeneration = 2; state.Ready = false;
        directory.Invalidate(state);
        Assert.Empty(live.Pending); Assert.Empty(live.Participants); Assert.False(participant.IsOpen);
        Assert.Throws<ApiException>(() => directory.Control(live, participant, message.RootElement));
        await using var replacement = Peer(); directory.RegisterHost(state, replacement);
        Assert.False(host.IsOpen);
        Assert.Throws<ApiException>(() => directory.Output(live, host, Frame(3, 2, 1, 2, 0)));
    }

    [Fact]
    public async Task QueueSaturationClosesInsteadOfAccumulatingWithoutLimit()
    {
        var socket = new FakeSocket(blockWrites: true);
        await using var peer = new SocketPeer(socket, Guid.CreateVersion7(), "device", Guid.CreateVersion7().ToString());
        var accepted = 0;
        for (var i = 0; i < 100; i++) if (peer.SendBinary(new byte[1024 * 1024])) accepted++;
        Assert.InRange(accepted, 1, 22); Assert.False(peer.IsOpen);
    }

    [Fact]
    public async Task CheckpointDeadlineRetriesAreFiniteAndDisconnectClearsRuntimePresence()
    {
        var clock = new ManualClock(); var directory = new RelayDirectory(clock);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        await using var peer = Peer(); directory.RegisterParticipant(state, peer); directory.BeginBootstrap(live, peer, Challenge());
        for (var i = 0; i < 3; i++) { clock.Advance(TimeSpan.FromSeconds(6)); directory.Sweep(); }
        Assert.False(peer.IsOpen); Assert.Empty(live.Checkpoints);
        directory.RemovePeer(live, host, host: true); Assert.False(directory.HostOnline(state.Id));
        await using var replacement = Peer(); var next = directory.RegisterHost(state, replacement);
        Assert.NotSame(live, next);
    }

    [Fact]
    public async Task ControlResultsCannotTargetAnotherConnectionAndExpiredControlsClose()
    {
        var clock = new ManualClock(); var directory = new RelayDirectory(clock);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        await using var peer = Peer(); directory.RegisterParticipant(state, peer); directory.BeginBootstrap(live, peer, Challenge());
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 0, 1, 0), Signature);
        var requestId = Guid.CreateVersion7();
        using var control = JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            type = "control",
            sequence = 1,
            keyGeneration = 1,
            requestId,
            nonce = Convert.ToBase64String(new byte[12]),
            ciphertext = Convert.ToBase64String(new byte[32]),
            signature = Convert.ToBase64String(new byte[3309])
        }));
        directory.Control(live, peer, control.RootElement);
        using var wrong = JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            type = "controlResult",
            requestId,
            sequence = 1,
            connectionId = "other",
            accepted = true,
            signature = Convert.ToBase64String(new byte[3309])
        }));
        Assert.Throws<ApiException>(() => directory.Result(live, host, wrong.RootElement)); Assert.Single(live.Pending);
        clock.Advance(TimeSpan.FromSeconds(11)); directory.Sweep();
        Assert.Empty(live.Pending); Assert.False(peer.IsOpen);
    }

    [Fact]
    public async Task CheckpointProofIsBoundToThePendingViewerAndQueuedBeforeItsExactFrame()
    {
        var directory = new RelayDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 3, KeyGeneration = 2, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        var capture = new CapturingSocket();
        await using var viewer = new SocketPeer(capture, Guid.CreateVersion7(), "viewer-device", Guid.CreateVersion7().ToString());
        directory.RegisterParticipant(state, viewer);
        Assert.Throws<ApiException>(() => directory.BeginBootstrap(live, viewer, Convert.ToBase64String(new byte[31])));
        var challenge = Challenge(); directory.BeginBootstrap(live, viewer, challenge);
        var request = live.Checkpoints.Keys.Single(); var frame = Frame(3, 2, 4, 7, 11);
        Assert.Throws<ApiException>(() => directory.Checkpoint(live, host, request, frame, Convert.ToBase64String(new byte[3308])));
        Assert.False(live.Participants[viewer.ConnectionId].Ready);
        directory.Checkpoint(live, host, request, frame, Signature);
        var proofFrame = await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        var snapshotFrame = await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        Assert.Equal(WebSocketMessageType.Text, proofFrame.Type); Assert.Equal(WebSocketMessageType.Binary, snapshotFrame.Type);
        Assert.Equal(frame, snapshotFrame.Bytes);
        using var proof = JsonDocument.Parse(proofFrame.Bytes);
        Assert.Equal("checkpointProof", proof.RootElement.GetProperty("type").GetString());
        Assert.Equal(challenge, proof.RootElement.GetProperty("challenge").GetString());
        Assert.Equal(viewer.ConnectionId, proof.RootElement.GetProperty("connectionId").GetString());
        Assert.Equal(viewer.UserId, proof.RootElement.GetProperty("recipientUserId").GetGuid());
        Assert.Equal(viewer.DeviceId, proof.RootElement.GetProperty("recipientDeviceId").GetString());
        Assert.Equal(Convert.ToBase64String(System.Security.Cryptography.SHA256.HashData(frame)), proof.RootElement.GetProperty("frameSha256").GetString());
        Assert.Throws<ApiException>(() => directory.RequestCapture(live, viewer, challenge));
        directory.Checkpoint(live, host, request, frame, Signature);
        Assert.False(capture.Frames.Reader.TryRead(out _));
    }

    [Fact]
    public async Task PresenceIsReportedAfterBootstrapAndRemovedOnDisconnect()
    {
        var directory = new RelayDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        var capture = new CapturingSocket();
        await using var host = new SocketPeer(capture, Guid.CreateVersion7(), "host", Guid.CreateVersion7().ToString());
        var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        await using var viewer = Peer(); directory.RegisterParticipant(state, viewer);
        Assert.False(capture.Frames.Reader.TryRead(out _));
        directory.BeginBootstrap(live, viewer, Challenge());
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 0, 1, 0), Signature);
        var connected = await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        using var presence = JsonDocument.Parse(connected.Bytes);
        Assert.Equal("participantConnected", presence.RootElement.GetProperty("type").GetString());
        Assert.Equal(viewer.UserId, presence.RootElement.GetProperty("senderUserId").GetGuid());
        directory.RemovePeer(live, viewer, host: false);
        var disconnected = await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        using var removed = JsonDocument.Parse(disconnected.Bytes);
        Assert.Equal("participantDisconnected", removed.RootElement.GetProperty("type").GetString());
    }

    [Fact]
    public async Task MetadataHintsOnlyReachReadyViewersFromTheCurrentHost()
    {
        var directory = new RelayDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        var capture = new CapturingSocket();
        await using var viewer = new SocketPeer(capture, Guid.CreateVersion7(), "viewer", Guid.CreateVersion7().ToString());
        directory.RegisterParticipant(state, viewer);
        directory.MetadataChanged(live, host);
        Assert.False(capture.Frames.Reader.TryRead(out _));
        directory.BeginBootstrap(live, viewer, Challenge());
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 0, 1, 0), Signature);
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        directory.MetadataChanged(live, host);
        var hint = await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        using var json = JsonDocument.Parse(hint.Bytes);
        Assert.Equal("metadataChanged", json.RootElement.GetProperty("type").GetString());
        Assert.Single(json.RootElement.EnumerateObject());
        await using var other = Peer();
        Assert.Throws<ApiException>(() => directory.MetadataChanged(live, other));
    }

    [Fact]
    public async Task TerminalEndDrainsOutputBeforeTheFinalBoundary()
    {
        var directory = new RelayDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        var capture = new CapturingSocket();
        await using var viewer = new SocketPeer(capture, Guid.CreateVersion7(), "viewer", Guid.CreateVersion7().ToString());
        directory.RegisterParticipant(state, viewer); directory.BeginBootstrap(live, viewer, Challenge());
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 0, 1, 0), Signature);
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        var output = Frame(4, 1, 0, 0, 1);
        directory.Output(live, host, output);
        await directory.EndSessionAsync(state.Id, host, 1);
        var raw = await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        var end = await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        Assert.Equal(output, raw.Bytes);
        using var message = JsonDocument.Parse(end.Bytes);
        Assert.Equal("ended", message.RootElement.GetProperty("type").GetString());
        Assert.Equal(1UL, message.RootElement.GetProperty("finalSequence").GetUInt64());
        var firstClose = viewer.DrainAndCloseAsync(new { type = "ended", finalSequence = 1 });
        var repeatedClose = viewer.DrainAndCloseAsync(new { type = "ended", finalSequence = 1 });
        Assert.Same(firstClose, repeatedClose);
        await repeatedClose;
        Assert.False(capture.Frames.Reader.TryRead(out _));
    }

    [Fact]
    public async Task HeartbeatsDoNotAbortAcceptedOutputWhileDraining()
    {
        var socket = new PausedSocket();
        await using var peer = new SocketPeer(socket, Guid.CreateVersion7(), "device", Guid.CreateVersion7().ToString());
        var first = new byte[] { 1 };
        var second = new byte[] { 2 };
        Assert.True(peer.SendBinary(first));
        await socket.Started.Task.WaitAsync(TimeSpan.FromSeconds(2), TestContext.Current.CancellationToken);
        Assert.True(peer.SendBinary(second));
        var drain = peer.DrainAndCloseAsync(new { type = "ended", finalSequence = 2 });
        Assert.False(peer.Send(new { type = "ping" }));
        Assert.False(peer.Send(new { type = "pong" }));
        Assert.True(peer.IsOpen);
        Assert.False(drain.IsCompleted);
        socket.Release.TrySetResult();
        await drain.WaitAsync(TimeSpan.FromSeconds(2), TestContext.Current.CancellationToken);
        Assert.Equal(first, (await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes);
        Assert.Equal(second, (await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes);
        using var ended = JsonDocument.Parse((await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes);
        Assert.Equal("ended", ended.RootElement.GetProperty("type").GetString());
        Assert.False(socket.Frames.Reader.TryRead(out _));
    }

    [Fact]
    public async Task RevocationCanStillAbortADrainingPeer()
    {
        var socket = new PausedSocket();
        await using var peer = new SocketPeer(socket, Guid.CreateVersion7(), "device", Guid.CreateVersion7().ToString());
        peer.SendBinary(new byte[] { 1 });
        await socket.Started.Task.WaitAsync(TimeSpan.FromSeconds(2), TestContext.Current.CancellationToken);
        var drain = peer.DrainAndCloseAsync(new { type = "ended", finalSequence = 1 });
        peer.Abort();
        await drain.WaitAsync(TimeSpan.FromSeconds(2), TestContext.Current.CancellationToken);
        Assert.False(peer.IsOpen);
        Assert.False(socket.Frames.Reader.TryRead(out _));
    }

    private sealed class PausedSocket : FakeSocket
    {
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public System.Threading.Channels.Channel<(WebSocketMessageType Type, byte[] Bytes)> Frames { get; } =
            System.Threading.Channels.Channel.CreateUnbounded<(WebSocketMessageType, byte[])>();
        public override async Task SendAsync(ArraySegment<byte> buffer, WebSocketMessageType type, bool endOfMessage, CancellationToken ct)
        {
            Started.TrySetResult();
            await Release.Task.WaitAsync(ct);
            Frames.Writer.TryWrite((type, buffer.ToArray()));
        }
    }

    [Fact]
    public async Task FailedMutationRecoveryDisconnectsOnlyAffectedHosts()
    {
        var directory = new RelayDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); directory.RegisterHost(state, host); directory.MarkReady(state);
        var unrelated = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7() };
        await using var rotatingHost = Peer(); directory.RegisterHost(unrelated, rotatingHost);
        using (var failedValidation = directory.BeginMutation()) failedValidation.Recover();
        Assert.True(host.IsOpen); Assert.True(rotatingHost.IsOpen);
        using var failedMutation = directory.BeginMutation();
        directory.Invalidate(state, notifyHost: false);
        await Task.Yield();
        failedMutation.Recover();
        Assert.False(host.IsOpen); Assert.True(rotatingHost.IsOpen);
    }

    private sealed class CapturingSocket : FakeSocket
    {
        public System.Threading.Channels.Channel<(WebSocketMessageType Type, byte[] Bytes)> Frames { get; } =
            System.Threading.Channels.Channel.CreateUnbounded<(WebSocketMessageType, byte[])>();
        public override Task SendAsync(ArraySegment<byte> buffer, WebSocketMessageType type, bool endOfMessage, CancellationToken ct)
        { Frames.Writer.TryWrite((type, buffer.ToArray())); return Task.CompletedTask; }
    }

    [Fact]
    public async Task ReadyCaptureTimeoutDoesNotInterruptExistingOutput()
    {
        var clock = new ManualClock(); var directory = new RelayDirectory(clock);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        await using var viewer = Peer(); directory.RegisterParticipant(state, viewer);
        directory.BeginBootstrap(live, viewer, Challenge());
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 0, 1, 0), Signature);
        directory.RequestCapture(live, viewer, Challenge()); var late = live.Checkpoints.Keys.Single();
        clock.Advance(TimeSpan.FromSeconds(16)); directory.Sweep();
        Assert.Empty(live.Checkpoints); Assert.True(viewer.IsOpen); Assert.True(live.Participants[viewer.ConnectionId].Ready);
        directory.Output(live, host, Frame(4, 1, 0, 0, 1));
        directory.Checkpoint(live, host, late, Frame(3, 1, 1, 2, 0), Signature);
        Assert.Equal(1UL, live.Participants[viewer.ConnectionId].NextSequence);
        Assert.True(viewer.IsOpen);
    }

    private static string Challenge() => Convert.ToBase64String(System.Security.Cryptography.RandomNumberGenerator.GetBytes(32));
    private static string Signature => Convert.ToBase64String(new byte[3309]);
    private static SocketPeer Peer() => new(new FakeSocket(), Guid.CreateVersion7(), "device", Guid.CreateVersion7().ToString());
    private sealed class ManualClock : TimeProvider
    {
        private DateTimeOffset now = DateTimeOffset.UtcNow;
        public override DateTimeOffset GetUtcNow() => now;
        public void Advance(TimeSpan amount) => now += amount;
    }
}

internal class FakeSocket(bool blockWrites = false) : WebSocket
{
    private WebSocketState state = WebSocketState.Open;
    public override WebSocketCloseStatus? CloseStatus => null;
    public override string? CloseStatusDescription => null;
    public override WebSocketState State => state;
    public override string? SubProtocol => null;
    public override void Abort() => state = WebSocketState.Aborted;
    public override Task CloseAsync(WebSocketCloseStatus closeStatus, string? statusDescription, CancellationToken cancellationToken) { state = WebSocketState.Closed; return Task.CompletedTask; }
    public override Task CloseOutputAsync(WebSocketCloseStatus closeStatus, string? statusDescription, CancellationToken cancellationToken) => CloseAsync(closeStatus, statusDescription, cancellationToken);
    public override void Dispose() => state = WebSocketState.Closed;
    public override Task<WebSocketReceiveResult> ReceiveAsync(ArraySegment<byte> buffer, CancellationToken cancellationToken) => throw new NotSupportedException();
    public override Task SendAsync(ArraySegment<byte> buffer, WebSocketMessageType messageType, bool endOfMessage, CancellationToken cancellationToken)
        => blockWrites ? Task.Delay(Timeout.Infinite, cancellationToken) : Task.CompletedTask;
}
