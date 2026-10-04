using System.Net.WebSockets;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.TerminalConnections;
using Xunit;

namespace Kodosi.HostTests;

public sealed class TerminalConnectionTests
{
    private static Session Terminal() => new() { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1 };
    private static Peer Viewer(Guid? user = null, string device = "viewer") => new(new FakeSocket(), user ?? Guid.CreateVersion7(), device, Guid.CreateVersion7().ToString());

    private static async Task<JsonElement> MessageAsync(CapturingSocket socket)
    {
        var frame = await socket.Frames.Reader.ReadAsync(TestContext.Current.CancellationToken);
        Assert.Equal(WebSocketMessageType.Text, frame.Type);
        using var json = JsonDocument.Parse(frame.Bytes);
        return json.RootElement.Clone();
    }

    [Fact]
    public async Task TheHostIsToldWhoConnectsAndOnlyItsOwnDeviceCanJoinThatPipeOnce()
    {
        var directory = new ConnectionDirectory(); var state = Terminal();
        var owner = Guid.CreateVersion7(); var hostSocket = new CapturingSocket();
        await using var host = new SocketPeer(hostSocket, owner, "host", Guid.CreateVersion7().ToString());
        directory.RegisterHost(state, host);
        Assert.True(directory.HostOnline(state.Id));
        await using var viewer = Viewer();
        var (_, pipe) = directory.OpenPipe(state, viewer);
        var notice = await MessageAsync(hostSocket);
        Assert.Equal("viewer", notice.GetProperty("type").GetString());
        Assert.Equal(pipe.Id, notice.GetProperty("channelId").GetGuid());
        Assert.Equal(viewer.UserId, notice.GetProperty("userId").GetGuid());
        Assert.Equal("viewer", notice.GetProperty("deviceId").GetString());
        Assert.False(pipe.Joined.Task.IsCompleted);

        await using var other = Viewer(owner, "other-device");
        Assert.Equal(404, Assert.Throws<ApiException>(() => directory.JoinPipe(state, pipe.Id, other)).Status);
        await using var stranger = Viewer(device: "host");
        Assert.Equal(404, Assert.Throws<ApiException>(() => directory.JoinPipe(state, pipe.Id, stranger)).Status);
        await using var relay = Viewer(owner, "host");
        Assert.Equal(404, Assert.Throws<ApiException>(() => directory.JoinPipe(state, Guid.CreateVersion7(), relay)).Status);
        directory.JoinPipe(state, pipe.Id, relay);
        Assert.Same(relay, await pipe.Joined.Task);
        await using var second = Viewer(owner, "host");
        Assert.Equal(404, Assert.Throws<ApiException>(() => directory.JoinPipe(state, pipe.Id, second)).Status);
    }

    [Fact]
    public async Task AnOfflineOrFullTerminalRefusesViewers()
    {
        var directory = new ConnectionDirectory(); var state = Terminal();
        await using var early = Viewer();
        Assert.Equal(409, Assert.Throws<ApiException>(() => directory.OpenPipe(state, early)).Status);
        await using var host = new SocketPeer(new FakeSocket(), Guid.CreateVersion7(), "host", Guid.CreateVersion7().ToString());
        directory.RegisterHost(state, host);
        var replaced = new Session { Id = state.Id, IncarnationId = Guid.CreateVersion7() };
        Assert.Equal(409, Assert.Throws<ApiException>(() => directory.OpenPipe(replaced, early)).Status);
        var viewers = Enumerable.Range(0, 64).Select(_ => Viewer()).ToArray();
        foreach (var viewer in viewers) directory.OpenPipe(state, viewer);
        Assert.Equal(503, Assert.Throws<ApiException>(() => directory.OpenPipe(state, early)).Status);
        foreach (var viewer in viewers) await viewer.DisposeAsync();
    }

    [Fact]
    public async Task RevocationClosesOnlyThePipesOfRemovedPeopleAndTellsTheHost()
    {
        var directory = new ConnectionDirectory(); var state = Terminal();
        var owner = Guid.CreateVersion7(); var hostSocket = new CapturingSocket();
        await using var host = new SocketPeer(hostSocket, owner, "host", Guid.CreateVersion7().ToString());
        directory.RegisterHost(state, host);
        await using var kept = Viewer(); await using var removed = Viewer();
        await using var keptRelay = Viewer(owner, "host"); await using var removedRelay = Viewer(owner, "host");
        var (_, keptPipe) = directory.OpenPipe(state, kept); directory.JoinPipe(state, keptPipe.Id, keptRelay);
        var (_, removedPipe) = directory.OpenPipe(state, removed); directory.JoinPipe(state, removedPipe.Id, removedRelay);
        await MessageAsync(hostSocket); await MessageAsync(hostSocket);

        directory.Revoke(state, (user, _) => user != removed.UserId);
        Assert.True(kept.IsOpen); Assert.True(keptRelay.IsOpen); Assert.True(host.IsOpen);
        Assert.False(removed.IsOpen); Assert.False(removedRelay.IsOpen);
        Assert.Equal("accessChanged", (await MessageAsync(hostSocket)).GetProperty("type").GetString());
        await using var late = Viewer(owner, "host");
        Assert.Equal(404, Assert.Throws<ApiException>(() => directory.JoinPipe(state, removedPipe.Id, late)).Status);
    }

    [Fact]
    public async Task ARemovedDeviceLosesItsPipesAndItsHostedTerminals()
    {
        var directory = new ConnectionDirectory(); var hosted = Terminal(); var watched = Terminal();
        var user = Guid.CreateVersion7();
        await using var lostHost = new SocketPeer(new FakeSocket(), user, "lost", Guid.CreateVersion7().ToString());
        await using var otherHost = new SocketPeer(new FakeSocket(), Guid.CreateVersion7(), "other", Guid.CreateVersion7().ToString());
        directory.RegisterHost(hosted, lostHost); directory.RegisterHost(watched, otherHost);
        await using var guest = Viewer(); directory.OpenPipe(hosted, guest);
        await using var lostViewer = Viewer(user, "lost"); await using var keptViewer = Viewer(user, "kept");
        directory.OpenPipe(watched, lostViewer); directory.OpenPipe(watched, keptViewer);

        directory.RemoveDevice(user, "lost");
        Assert.False(lostHost.IsOpen); Assert.False(lostViewer.IsOpen);
        Assert.True(otherHost.IsOpen); Assert.True(keptViewer.IsOpen);
        Assert.False(directory.HostOnline(hosted.Id)); Assert.True(directory.HostOnline(watched.Id));
    }

    [Fact]
    public async Task ANewHostConnectionReplacesTheOldOneAndEndsItsPipes()
    {
        var directory = new ConnectionDirectory(); var state = Terminal(); var owner = Guid.CreateVersion7();
        await using var first = new SocketPeer(new FakeSocket(), owner, "host", Guid.CreateVersion7().ToString());
        var old = directory.RegisterHost(state, first);
        await using var viewer = Viewer(); directory.OpenPipe(state, viewer);
        await using var second = new SocketPeer(new FakeSocket(), owner, "host", Guid.CreateVersion7().ToString());
        var current = directory.RegisterHost(state, second);
        Assert.False(first.IsOpen); Assert.False(viewer.IsOpen); Assert.True(second.IsOpen);
        directory.RemoveHost(old);
        Assert.True(directory.HostOnline(state.Id));
        directory.RemoveHost(current);
        Assert.False(directory.HostOnline(state.Id)); Assert.False(second.IsOpen);
        var replaced = new Session { Id = state.Id, IncarnationId = Guid.CreateVersion7() };
        await using var third = new SocketPeer(new FakeSocket(), owner, "host", Guid.CreateVersion7().ToString());
        directory.RegisterHost(state, third);
        await using var wrong = new SocketPeer(new FakeSocket(), owner, "host", Guid.CreateVersion7().ToString());
        Assert.Equal(409, Assert.Throws<ApiException>(() => directory.RegisterHost(replaced, wrong)).Status);
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
