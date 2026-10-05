using System.Net.WebSockets;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.TerminalConnections;
using Xunit;
using DeviceLink = Kodosi.TerminalConnections.DeviceLink;

namespace Kodosi.HostTests;

public sealed class TerminalConnectionTests
{
    private static Session Terminal() => new() { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1 };
    private static (DeviceLink Link, CapturingSocket Socket) Device(Guid? user = null, string device = "viewer")
    {
        var socket = new CapturingSocket();
        return (new DeviceLink(socket, user ?? Guid.CreateVersion7(), device, Guid.CreateVersion7().ToString()), socket);
    }

    private static async Task<JsonElement> MessageAsync(CapturingSocket socket, string type)
    {
        while (true)
        {
            var frame = await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken).AsTask()
                .WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
            if (frame.Type != WebSocketMessageType.Text) continue;
            using var json = JsonDocument.Parse(frame.Bytes);
            if (json.RootElement.GetProperty("type").GetString() == type) return json.RootElement.Clone();
        }
    }

    private static async Task<byte[]> BinaryAsync(CapturingSocket socket)
    {
        while (true)
        {
            var frame = await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken).AsTask()
                .WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
            if (frame.Type == WebSocketMessageType.Binary) return frame.Bytes;
        }
    }

    [Fact]
    public async Task TheHostIsToldWhoConnectsAndOnlyTheTwoEndsOfAPipeCanUseIt()
    {
        var directory = new ConnectionDirectory(); var state = Terminal();
        var (host, hostSocket) = Device(device: "host"); var (viewer, viewerSocket) = Device(); var (other, otherSocket) = Device(device: "other");
        await using var disposeHost = host; await using var disposeViewer = viewer; await using var disposeOther = other;
        directory.Register(host); directory.Register(viewer); directory.Register(other);
        directory.Host(host, state);
        Assert.True(directory.HostOnline(state.Id));
        var pipe = Guid.CreateVersion7();
        directory.Open(viewer, pipe, state);
        var notice = await MessageAsync(hostSocket, "viewer");
        Assert.Equal(pipe, notice.GetProperty("pipe").GetGuid());
        Assert.Equal(state.Id, notice.GetProperty("sessionId").GetGuid());
        Assert.Equal(viewer.UserId, notice.GetProperty("userId").GetGuid());
        Assert.Equal("viewer", notice.GetProperty("deviceId").GetString());
        Assert.Equal(409, Assert.Throws<ApiException>(() => directory.Open(other, pipe, state)).Status);

        var up = BackendApplication.PipeFrame(pipe, [1, 2]); var down = BackendApplication.PipeFrame(pipe, [3]);
        Assert.Equal(up.Length, directory.Relay(viewer, pipe, up));
        Assert.Equal(up, await BinaryAsync(hostSocket));
        Assert.Equal(down.Length, directory.Relay(host, pipe, down));
        Assert.Equal(down, await BinaryAsync(viewerSocket));
        Assert.Equal(0, directory.Relay(other, pipe, up));
        Assert.Equal(0, directory.Relay(viewer, Guid.CreateVersion7(), up));
        directory.Close(other, pipe);
        Assert.Equal(up.Length, directory.Relay(viewer, pipe, up));

        directory.Close(viewer, pipe);
        Assert.Equal(pipe, (await MessageAsync(hostSocket, "closed")).GetProperty("pipe").GetGuid());
        Assert.Equal(0, directory.Relay(host, pipe, down));
        Assert.False(otherSocket.Frames.Reader.TryRead(out _));
    }

    [Fact]
    public async Task AnOfflineOrFullTerminalRefusesViewers()
    {
        var directory = new ConnectionDirectory(); var state = Terminal();
        var (viewer, _) = Device(); await using var disposeViewer = viewer; directory.Register(viewer);
        Assert.Equal(409, Assert.Throws<ApiException>(() => directory.Open(viewer, Guid.CreateVersion7(), state)).Status);
        var (host, _) = Device(device: "host"); await using var disposeHost = host; directory.Register(host);
        directory.Host(host, state);
        var replaced = new Session { Id = state.Id, IncarnationId = Guid.CreateVersion7() };
        Assert.Equal(409, Assert.Throws<ApiException>(() => directory.Open(viewer, Guid.CreateVersion7(), replaced)).Status);
        for (var index = 0; index < 64; index++) directory.Open(viewer, Guid.CreateVersion7(), state);
        Assert.Equal(503, Assert.Throws<ApiException>(() => directory.Open(viewer, Guid.CreateVersion7(), state)).Status);
        directory.Unhost(viewer, state.Id);
        Assert.True(directory.HostOnline(state.Id));
        directory.Unhost(host, state.Id);
        Assert.False(directory.HostOnline(state.Id));
        Assert.Equal((2, 0), directory.Count());
    }

    [Fact]
    public async Task RevocationClosesOnlyThePipesOfRemovedPeopleAndTellsTheHost()
    {
        var directory = new ConnectionDirectory(); var state = Terminal();
        var (host, hostSocket) = Device(device: "host"); var (kept, keptSocket) = Device(); var (removed, removedSocket) = Device();
        await using var disposeHost = host; await using var disposeKept = kept; await using var disposeRemoved = removed;
        directory.Register(host); directory.Register(kept); directory.Register(removed); directory.Host(host, state);
        var keptPipe = Guid.CreateVersion7(); var removedPipe = Guid.CreateVersion7();
        directory.Open(kept, keptPipe, state); directory.Open(removed, removedPipe, state);

        directory.Revoke(state, (user, _) => user != removed.UserId);
        Assert.Equal(403, (await MessageAsync(removedSocket, "closed")).GetProperty("status").GetInt32());
        Assert.Equal(removedPipe, (await MessageAsync(hostSocket, "closed")).GetProperty("pipe").GetGuid());
        Assert.Equal(state.Id, (await MessageAsync(hostSocket, "accessChanged")).GetProperty("sessionId").GetGuid());
        Assert.True(kept.IsOpen); Assert.True(removed.IsOpen); Assert.True(host.IsOpen);
        var frame = BackendApplication.PipeFrame(keptPipe, [7]);
        Assert.Equal(frame.Length, directory.Relay(host, keptPipe, frame));
        Assert.Equal(frame, await BinaryAsync(keptSocket));
        Assert.Equal(0, directory.Relay(removed, removedPipe, BackendApplication.PipeFrame(removedPipe, [7])));
    }

    [Fact]
    public async Task ARemovedOrReplacedDeviceConnectionLosesItsPipesAndItsHostedTerminals()
    {
        var directory = new ConnectionDirectory(); var hosted = Terminal(); var watched = Terminal();
        var user = Guid.CreateVersion7();
        var (lost, _) = Device(user, "lost"); var (otherHost, otherSocket) = Device(device: "other"); var (guest, guestSocket) = Device(); var (keptViewer, _) = Device(user, "kept");
        await using var disposeLost = lost; await using var disposeOther = otherHost; await using var disposeGuest = guest; await using var disposeKept = keptViewer;
        directory.Register(lost); directory.Register(otherHost); directory.Register(guest); directory.Register(keptViewer);
        directory.Host(lost, hosted); directory.Host(otherHost, watched);
        var guestPipe = Guid.CreateVersion7(); var lostPipe = Guid.CreateVersion7(); var keptPipe = Guid.CreateVersion7();
        directory.Open(guest, guestPipe, hosted); directory.Open(lost, lostPipe, watched); directory.Open(keptViewer, keptPipe, watched);

        directory.RemoveDevice(user, "lost");
        Assert.False(lost.IsOpen); Assert.True(otherHost.IsOpen); Assert.True(keptViewer.IsOpen); Assert.True(guest.IsOpen);
        Assert.False(directory.HostOnline(hosted.Id)); Assert.True(directory.HostOnline(watched.Id));
        Assert.Equal(409, (await MessageAsync(guestSocket, "closed")).GetProperty("status").GetInt32());
        Assert.Equal(lostPipe, (await MessageAsync(otherSocket, "closed")).GetProperty("pipe").GetGuid());
        Assert.Equal((3, 1), directory.Count());

        var (again, _) = Device(otherHost.UserId, "other"); await using var disposeAgain = again;
        directory.Register(again);
        Assert.False(otherHost.IsOpen); Assert.True(again.IsOpen);
        Assert.False(directory.HostOnline(watched.Id));
        Assert.Equal((3, 0), directory.Count());
        directory.Remove(otherHost);
        Assert.True(again.IsOpen);
        directory.Host(again, watched);
        Assert.True(directory.HostOnline(watched.Id));
    }

    [Fact]
    public async Task APipeThatIsTooSlowIsClosedAndTheDeviceConnectionStaysOpen()
    {
        var directory = new ConnectionDirectory(); var state = Terminal();
        var (host, hostSocket) = Device(device: "host"); await using var disposeHost = host;
        await using var slow = new DeviceLink(new FakeSocket(blockWrites: true), Guid.CreateVersion7(), "slow", Guid.CreateVersion7().ToString());
        directory.Register(host); directory.Register(slow); directory.Host(host, state);
        var pipe = Guid.CreateVersion7();
        directory.Open(slow, pipe, state);
        var frame = BackendApplication.PipeFrame(pipe, new byte[256 * 1024]);
        var relayed = 0;
        while (directory.Relay(host, pipe, frame) > 0) relayed++;
        Assert.InRange(relayed, 32, 80);
        Assert.Equal(pipe, (await MessageAsync(hostSocket, "closed")).GetProperty("pipe").GetGuid());
        Assert.True(slow.IsOpen); Assert.True(host.IsOpen);
    }

    private sealed class CapturingSocket : FakeSocket
    {
        public System.Threading.Channels.Channel<(WebSocketMessageType Type, byte[] Bytes)> Frames { get; } =
            System.Threading.Channels.Channel.CreateUnbounded<(WebSocketMessageType, byte[])>();
        public override Task SendAsync(ArraySegment<byte> buffer, WebSocketMessageType type, bool endOfMessage, CancellationToken ct)
        { Frames.Writer.TryWrite((type, buffer.ToArray())); return Task.CompletedTask; }
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
