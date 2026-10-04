using System.Buffers.Binary;
using System.Net.WebSockets;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.TerminalConnections;
using Xunit;

namespace Kodosi.HostTests;

public sealed class TerminalConnectionTests
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
    public void OutputOrderAcceptsContiguousOutputAndSmallNoticesAndRejectsOtherFrames()
    {
        static OutputOrder.FrameHeader Header(byte type, ulong counter, ulong first, ulong next) => OutputOrder.Header(Frame(type, 1, counter, first, next), 1)!.Value;
        var order = new OutputOrder(); order.Accept(Header(4, 0, 10, 11));
        Assert.Throws<ApiException>(() => order.Accept(Header(4, 0, 11, 12)));
        Assert.Throws<ApiException>(() => order.Accept(Header(4, 1, 9, 10)));
        Assert.Throws<ApiException>(() => order.Accept(Header(3, 1, 1, 11)));
        order.Accept(Header(5, 0, 1, 0));
        Assert.Throws<ApiException>(() => order.Accept(Header(5, 0, 1, 0)));
        Assert.Throws<ApiException>(() => { OutputOrder.Header(Frame(5, 1, 1, 1, 0, OutputOrder.MaximumNoticeFrame + 1), 1); });
        Assert.Throws<ApiException>(() => { OutputOrder.Header(Frame(2, 1, 1, 2, 11), 1); });
        Assert.Throws<ApiException>(() => { OutputOrder.Header(Frame(6, 1, 1, 2, 11), 1); });
        Assert.Throws<ApiException>(() => { OutputOrder.Header(Frame(4, 2, 1, 11, 12), 1); });
        Assert.Null(OutputOrder.Header(Frame(4, 1, 1, 11, 12), 2));
        Assert.Equal(11UL, order.NextSequence);
        Assert.True(order.Follows(Header(3, 0, 1, 11))); Assert.False(order.Follows(Header(3, 0, 1, 10)));
        order.Accept(Header(4, 1, 40, 41));
        Assert.Equal(41UL, order.NextSequence);
        order.Restart(); order.Accept(Header(4, 2, 5, 6));
        Assert.Equal(6UL, order.NextSequence);
    }

    [Fact]
    public async Task LateParticipantBootstrapDoesNotResetOtherViewers()
    {
        var directory = new ConnectionDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
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
        var directory = new ConnectionDirectory(TimeProvider.System);
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
            ciphertext = Convert.ToBase64String(new byte[32])
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
        var clock = new ManualClock(); var directory = new ConnectionDirectory(clock);
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
        var clock = new ManualClock(); var directory = new ConnectionDirectory(clock);
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
            ciphertext = Convert.ToBase64String(new byte[32])
        }));
        directory.Control(live, peer, control.RootElement);
        using var wrong = JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            type = "controlResult",
            requestId,
            sequence = 1,
            connectionId = "other",
            nonce = Convert.ToBase64String(new byte[12]),
            ciphertext = Convert.ToBase64String(new byte[40])
        }));
        Assert.Throws<ApiException>(() => directory.Result(live, host, wrong.RootElement)); Assert.Single(live.Pending);
        clock.Advance(TimeSpan.FromSeconds(11)); directory.Sweep();
        Assert.Empty(live.Pending); Assert.False(peer.IsOpen);
    }

    [Fact]
    public async Task CheckpointProofIsBoundToThePendingViewerAndQueuedBeforeItsExactFrame()
    {
        var directory = new ConnectionDirectory(TimeProvider.System);
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
        var directory = new ConnectionDirectory(TimeProvider.System);
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
    public async Task MetadataFramesOnlyReachReadyViewersFromTheCurrentHost()
    {
        var directory = new ConnectionDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        var capture = new CapturingSocket();
        await using var viewer = new SocketPeer(capture, Guid.CreateVersion7(), "viewer", Guid.CreateVersion7().ToString());
        directory.RegisterParticipant(state, viewer);
        directory.Output(live, host, Frame(5, 1, 0, 0, 0));
        Assert.False(capture.Frames.Reader.TryRead(out _));
        directory.BeginBootstrap(live, viewer, Challenge());
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 0, 1, 0), Signature);
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        var metadata = Frame(5, 1, 1, 1, 0);
        directory.Output(live, host, metadata);
        var forwarded = await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        Assert.Equal(WebSocketMessageType.Binary, forwarded.Type); Assert.Equal(metadata, forwarded.Bytes);
        var output = Frame(4, 1, 0, 0, 1);
        directory.Output(live, host, output);
        Assert.Equal(output, (await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes);
        await using var other = Peer();
        Assert.Throws<ApiException>(() => directory.Output(live, other, Frame(5, 1, 2, 1, 0)));
    }

    [Fact]
    public async Task HostResyncKeepsViewersConnectedUntilTheyAskForANewSnapshot()
    {
        var directory = new ConnectionDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        var capture = new CapturingSocket();
        await using var viewer = new SocketPeer(capture, Guid.CreateVersion7(), "viewer", Guid.CreateVersion7().ToString());
        directory.RegisterParticipant(state, viewer); directory.BeginBootstrap(live, viewer, Challenge());
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 0, 1, 10), Signature);
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        directory.BeginBootstrap(live, viewer, Challenge(), resync: true);
        Assert.Empty(live.Checkpoints); Assert.True(live.Participants[viewer.ConnectionId].Ready);

        directory.Resync(live, host);
        using (var resync = JsonDocument.Parse((await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes))
        {
            Assert.Equal("resync", resync.RootElement.GetProperty("type").GetString());
            Assert.Single(resync.RootElement.EnumerateObject());
        }
        Assert.True(viewer.IsOpen); Assert.False(live.Participants[viewer.ConnectionId].Ready);
        directory.Output(live, host, Frame(4, 1, 0, 40, 41));
        directory.RequestCapture(live, viewer, Challenge());
        Assert.Empty(live.Checkpoints);
        using var control = JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            type = "control",
            sequence = 1,
            keyGeneration = 1,
            requestId = Guid.CreateVersion7(),
            nonce = Convert.ToBase64String(new byte[12]),
            ciphertext = Convert.ToBase64String(new byte[32])
        }));
        directory.Control(live, viewer, control.RootElement);
        Assert.Single(live.Pending);

        directory.BeginBootstrap(live, viewer, Challenge(), resync: true);
        var snapshot = Frame(3, 1, 1, 2, 41);
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), snapshot, Signature);
        Assert.Equal(WebSocketMessageType.Text, (await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Type);
        Assert.Equal(snapshot, (await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes);
        Assert.True(live.Participants[viewer.ConnectionId].Ready);
        var output = Frame(4, 1, 1, 41, 42);
        directory.Output(live, host, output);
        Assert.Equal(output, (await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes);
        await using var other = Peer();
        Assert.Throws<ApiException>(() => directory.Resync(live, other));
    }

    [Fact]
    public async Task AccessChangeKeepsAllowedViewersConnectedAndTellsThemToTakeTheNewKey()
    {
        var directory = new ConnectionDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        var hostSocket = new CapturingSocket();
        await using var host = new SocketPeer(hostSocket, Guid.CreateVersion7(), "host", Guid.CreateVersion7().ToString());
        var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        var capture = new CapturingSocket();
        await using var kept = new SocketPeer(capture, Guid.CreateVersion7(), "kept", Guid.CreateVersion7().ToString());
        await using var removed = Peer();
        foreach (var viewer in new[] { kept, removed })
        {
            directory.RegisterParticipant(state, viewer); directory.BeginBootstrap(live, viewer, Challenge());
            directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 0, 1, 0), Signature);
        }
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        for (var i = 0; i < 4; i++) await hostSocket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        JsonDocument Control(ulong sequence, int generation) => JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            type = "control",
            sequence,
            keyGeneration = generation,
            requestId = Guid.CreateVersion7(),
            nonce = Convert.ToBase64String(new byte[12]),
            ciphertext = Convert.ToBase64String(new byte[32])
        }));
        using (var before = Control(1, 1)) directory.Control(live, kept, before.RootElement);
        await hostSocket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);

        state.AuthorizationRevision = 2; state.KeyGeneration = 2; state.Ready = false;
        directory.Invalidate(state, keep: (user, _) => user == kept.UserId);
        Assert.True(kept.IsOpen); Assert.False(removed.IsOpen); Assert.Equal(kept.ConnectionId, live.Participants.Keys.Single());
        Assert.Empty(live.Pending);
        using (var left = JsonDocument.Parse((await hostSocket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes))
        {
            Assert.Equal("participantDisconnected", left.RootElement.GetProperty("type").GetString());
            Assert.Equal(removed.ConnectionId, left.RootElement.GetProperty("connectionId").GetString());
        }
        using (var changed = JsonDocument.Parse((await hostSocket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes))
            Assert.Equal("accessChanged", changed.RootElement.GetProperty("type").GetString());
        directory.Output(live, host, Frame(4, 1, 0, 0, 1));
        using (var early = Control(2, 1)) directory.Control(live, kept, early.RootElement);
        Assert.Empty(live.Pending); Assert.False(capture.Frames.Reader.TryRead(out _));

        state.Ready = true; directory.MarkReady(state);
        using (var rekey = JsonDocument.Parse((await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes))
        {
            Assert.Equal("rekey", rekey.RootElement.GetProperty("type").GetString());
            Assert.Equal(2, rekey.RootElement.GetProperty("keyGeneration").GetInt32());
            Assert.Equal(2, rekey.RootElement.GetProperty("authorizationRevision").GetInt64());
        }
        directory.Output(live, host, Frame(4, 1, 1, 1, 2));
        using (var stale = Control(2, 1)) directory.Control(live, kept, stale.RootElement);
        directory.Output(live, host, Frame(4, 2, 0, 5, 6));
        Assert.Empty(live.Pending); Assert.True(host.IsOpen); Assert.True(kept.IsOpen);
        directory.BeginBootstrap(live, kept, Challenge(), resync: true);
        var snapshot = Frame(3, 2, 0, 1, 6);
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), snapshot, Signature);
        Assert.Equal(WebSocketMessageType.Text, (await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Type);
        Assert.Equal(snapshot, (await capture.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes);
        using (var after = Control(1, 2)) directory.Control(live, kept, after.RootElement);
        Assert.Single(live.Pending);
    }

    [Fact]
    public async Task AViewerThatFallsBehindGetsANewSnapshotWhileOtherViewersContinue()
    {
        var directory = new ConnectionDirectory(TimeProvider.System);
        var state = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1, KeyGeneration = 1, Ready = true };
        await using var host = Peer(); var live = directory.RegisterHost(state, host); directory.MarkReady(state);
        var prompt = new CapturingSocket();
        await using var current = new SocketPeer(prompt, Guid.CreateVersion7(), "current", Guid.CreateVersion7().ToString());
        directory.RegisterParticipant(state, current); directory.BeginBootstrap(live, current, Challenge());
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 0, 1, 0), Signature);
        await prompt.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        await prompt.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        var socket = new PausedSocket();
        await using var slow = new SocketPeer(socket, Guid.CreateVersion7(), "slow", Guid.CreateVersion7().ToString());
        directory.RegisterParticipant(state, slow); directory.BeginBootstrap(live, slow, Challenge());
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), Frame(3, 1, 1, 2, 0), Signature);
        await socket.Started.Task.WaitAsync(TimeSpan.FromSeconds(2), TestContext.Current.CancellationToken);
        for (ulong i = 0; i < 12; i++)
        {
            directory.Output(live, host, Frame(4, 1, i, i, i + 1, 1024 * 1024));
            await prompt.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
            current.Acknowledge(i + 1);
        }
        Assert.True(slow.IsOpen); Assert.False(live.Participants[slow.ConnectionId].Ready);
        Assert.True(live.Participants[current.ConnectionId].Ready);
        Assert.Equal(12UL, live.Participants[current.ConnectionId].NextSequence);

        socket.Release.TrySetResult();
        Assert.Equal(WebSocketMessageType.Text, (await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Type);
        using (var resync = JsonDocument.Parse((await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes))
            Assert.Equal("resync", resync.RootElement.GetProperty("type").GetString());
        directory.BeginBootstrap(live, slow, Challenge(), resync: true);
        var snapshot = Frame(3, 1, 2, 3, 12);
        directory.Checkpoint(live, host, live.Checkpoints.Keys.Single(), snapshot, Signature);
        Assert.Equal(WebSocketMessageType.Text, (await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Type);
        Assert.Equal(snapshot, (await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes);
        Assert.True(slow.IsOpen); Assert.Equal(12UL, live.Participants[slow.ConnectionId].NextSequence);
    }

    [Fact]
    public async Task OutputWaitsForTheViewerToAcknowledgeAndAnInputResultGoesFirst()
    {
        var socket = new CapturingSocket();
        await using var viewer = new SocketPeer(socket, Guid.CreateVersion7(), "viewer", Guid.CreateVersion7().ToString());
        var first = Frame(4, 1, 0, 0, 1, 32 * 1024); var second = Frame(4, 1, 1, 1, 2); var third = Frame(4, 1, 2, 2, 3);
        Assert.True(viewer.SendOutput(first, 1, () => true));
        Assert.Equal(first, (await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes);
        Assert.True(viewer.SendOutput(second, 2, () => true));
        Assert.True(viewer.SendOutput(third, 3, () => false));
        Assert.True(viewer.SendUrgent(new { type = "controlResult" }));
        Assert.Equal(WebSocketMessageType.Text, (await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Type);
        await Task.Delay(100, TestContext.Current.CancellationToken);
        Assert.False(socket.Frames.Reader.TryRead(out _));
        Assert.False(viewer.HasOutputRoom(64 * 1024));

        viewer.Acknowledge(1);
        Assert.Equal(second, (await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Bytes);
        Assert.True(viewer.Send(new { type = "resync" }));
        Assert.Equal(WebSocketMessageType.Text, (await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken)).Type);
        Assert.True(viewer.HasOutputRoom(1024 * 1024));
    }

    [Fact]
    public async Task TerminalEndDrainsOutputBeforeTheFinalBoundary()
    {
        var directory = new ConnectionDirectory(TimeProvider.System);
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
        var directory = new ConnectionDirectory(TimeProvider.System);
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
        var clock = new ManualClock(); var directory = new ConnectionDirectory(clock);
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
