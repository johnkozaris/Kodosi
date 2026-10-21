using System.Net.WebSockets;
using System.Text;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.TerminalConnections;
using Xunit;

namespace Kodosi.HostTests;

public sealed class TerminalConnectionTests
{
    private static Session Terminal() => new() { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), AuthorizationRevision = 1 };
    private static DeviceConnection Connection(WebSocket socket, Guid user, string device, RelayBudget relays) =>
        new(socket, user, device, relays, TimeProvider.System);

    private static (DeviceConnection Connection, CapturingSocket Socket) Device(ConnectionDirectory directory, Guid? user = null, string device = "viewer",
        bool hosts = false, int version = ContractVersions.TerminalConnection)
    {
        var socket = new CapturingSocket();
        return (new DeviceConnection(socket, user ?? Guid.CreateVersion7(), device, directory.Relays, TimeProvider.System, version, hosts), socket);
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
        var (host, hostSocket) = Device(directory, device: "host"); var (viewer, viewerSocket) = Device(directory); var (other, otherSocket) = Device(directory, device: "other");
        await using var disposeHost = host; await using var disposeViewer = viewer; await using var disposeOther = other;
        directory.Register(host); directory.Register(viewer); directory.Register(other);
        directory.Host(host, state);
        Assert.True(IsOnline(directory, state.Id));
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
        var (viewer, _) = Device(directory); await using var disposeViewer = viewer; directory.Register(viewer);
        Assert.Equal(409, Assert.Throws<ApiException>(() => directory.Open(viewer, Guid.CreateVersion7(), state)).Status);
        var (host, _) = Device(directory, device: "host"); await using var disposeHost = host; directory.Register(host);
        directory.Host(host, state);
        var replaced = new Session { Id = state.Id, IncarnationId = Guid.CreateVersion7() };
        Assert.Equal(409, Assert.Throws<ApiException>(() => directory.Open(viewer, Guid.CreateVersion7(), replaced)).Status);
        for (var index = 0; index < 64; index++) directory.Open(viewer, Guid.CreateVersion7(), state);
        Assert.Equal(503, Assert.Throws<ApiException>(() => directory.Open(viewer, Guid.CreateVersion7(), state)).Status);
        directory.Unhost(viewer, state.Id);
        Assert.True(IsOnline(directory, state.Id));
        directory.Unhost(host, state.Id);
        Assert.False(IsOnline(directory, state.Id));
        Assert.Equal((2, 0, 0), directory.Counts());
    }

    [Fact]
    public async Task RevocationClosesOnlyThePipesOfRemovedPeopleAndTellsTheHost()
    {
        var directory = new ConnectionDirectory(); var state = Terminal();
        var (host, hostSocket) = Device(directory, device: "host"); var (kept, keptSocket) = Device(directory); var (removed, removedSocket) = Device(directory);
        await using var disposeHost = host; await using var disposeKept = kept; await using var disposeRemoved = removed;
        directory.Register(host); directory.Register(kept); directory.Register(removed); directory.Host(host, state);
        var keptPipe = Guid.CreateVersion7(); var removedPipe = Guid.CreateVersion7();
        directory.Open(kept, keptPipe, state); directory.Open(removed, removedPipe, state);

        directory.Revoke(state.Id, (user, _) => user != removed.UserId);
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
        var (lost, _) = Device(directory, user, "lost"); var (otherHost, otherSocket) = Device(directory, device: "other"); var (guest, guestSocket) = Device(directory); var (keptViewer, _) = Device(directory, user, "kept");
        await using var disposeLost = lost; await using var disposeOther = otherHost; await using var disposeGuest = guest; await using var disposeKept = keptViewer;
        directory.Register(lost); directory.Register(otherHost); directory.Register(guest); directory.Register(keptViewer);
        directory.Host(lost, hosted); directory.Host(otherHost, watched);
        var guestPipe = Guid.CreateVersion7(); var lostPipe = Guid.CreateVersion7(); var keptPipe = Guid.CreateVersion7();
        directory.Open(guest, guestPipe, hosted); directory.Open(lost, lostPipe, watched); directory.Open(keptViewer, keptPipe, watched);

        directory.RemoveDevice(user, "lost");
        Assert.False(lost.IsOpen); Assert.True(otherHost.IsOpen); Assert.True(keptViewer.IsOpen); Assert.True(guest.IsOpen);
        Assert.False(IsOnline(directory, hosted.Id)); Assert.True(IsOnline(directory, watched.Id));
        Assert.Equal(409, (await MessageAsync(guestSocket, "closed")).GetProperty("status").GetInt32());
        Assert.Equal(lostPipe, (await MessageAsync(otherSocket, "closed")).GetProperty("pipe").GetGuid());
        Assert.Equal((3, 1, 1), directory.Counts());

        var (again, _) = Device(directory, otherHost.UserId, "other"); await using var disposeAgain = again;
        directory.Register(again);
        Assert.False(otherHost.IsOpen); Assert.True(again.IsOpen);
        Assert.False(IsOnline(directory, watched.Id));
        Assert.Equal((3, 0, 0), directory.Counts());
        directory.Remove(otherHost);
        Assert.True(again.IsOpen);
        directory.Host(again, watched);
        Assert.True(IsOnline(directory, watched.Id));
    }

    [Fact]
    public async Task APipeThatIsTooSlowIsClosedAndTheDeviceConnectionStaysOpen()
    {
        var directory = new ConnectionDirectory(); var state = Terminal();
        var (host, hostSocket) = Device(directory, device: "host"); await using var disposeHost = host;
        await using var slow = Connection(new FakeSocket(blockWrites: true), Guid.CreateVersion7(), "slow", directory.Relays);
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

    [Fact]
    public async Task AllRelayQueuesShareOneServerLimitThatAClosedConnectionFrees()
    {
        var budget = new RelayBudget(1024 * 1024);
        var first = Connection(new FakeSocket(blockWrites: true), Guid.CreateVersion7(), "first", budget);
        await using var second = Connection(new FakeSocket(blockWrites: true), Guid.CreateVersion7(), "second", budget);
        var frame = new byte[64 * 1024];
        var sent = 0;
        while (first.Relay(frame) == Relayed.Sent) sent++;
        Assert.Equal(16, sent);
        Assert.Equal(Relayed.Busy, second.Relay(frame));
        await first.DisposeAsync();
        Assert.Equal(0, budget.Used);
        Assert.Equal(Relayed.Sent, second.Relay(frame));
    }

    [Fact]
    public async Task APipeEndsWithABusyMessageWhenAllRelayQueuesAreFull()
    {
        var directory = new ConnectionDirectory(1024 * 1024); var state = Terminal();
        var (host, hostSocket) = Device(directory, device: "host"); await using var disposeHost = host;
        await using var slow = Connection(new FakeSocket(blockWrites: true), Guid.CreateVersion7(), "slow", directory.Relays);
        directory.Register(host); directory.Register(slow); directory.Host(host, state);
        var pipe = Guid.CreateVersion7();
        directory.Open(slow, pipe, state);
        var frame = BackendApplication.PipeFrame(pipe, new byte[64 * 1024]);
        while (directory.Relay(host, pipe, frame) > 0) { }
        var closed = await MessageAsync(hostSocket, "closed");
        Assert.Equal(pipe, closed.GetProperty("pipe").GetGuid());
        Assert.Equal("The Kodosi server is busy. Open the terminal again in a moment.", closed.GetProperty("message").GetString());
        Assert.True(slow.IsOpen); Assert.True(host.IsOpen);
    }

    [Fact]
    public async Task OnlyTheHostingConnectionSetsTheStatusAndTheStatusEndsWithIt()
    {
        var directory = new ConnectionDirectory(); var state = Terminal();
        var changed = new List<Guid>(); directory.HostChanged += changed.Add;
        var (host, _) = Device(directory, device: "host"); var (other, _) = Device(directory, device: "other");
        await using var disposeHost = host; await using var disposeOther = other;
        directory.Register(host); directory.Register(other); directory.Host(host, state);
        var status = new TerminalStatus("claude", "blocked", "permission");
        directory.Share(other, state.Id, state.IncarnationId, status);
        directory.Share(host, state.Id, Guid.CreateVersion7(), status);
        Assert.Null(directory.Live(state.Id).Status);
        directory.Share(host, state.Id, state.IncarnationId, status);
        directory.Share(host, state.Id, state.IncarnationId, new TerminalStatus("claude", "blocked", "permission"));
        Assert.Equal(status, directory.Live(state.Id).Status);
        directory.Remove(host);
        Assert.Null(directory.Live(state.Id).Status);
        Assert.False(IsOnline(directory, state.Id));
        Assert.Equal(Enumerable.Repeat(state.Id, 3), changed);
    }

    [Fact]
    public void ATerminalStatusCarriesOnlyTheAgentAndTheState()
    {
        static TerminalStatus? Read(string status) => TerminalStatus.Checked(((StatusMessage)Wire.Decode(Encoding.UTF8.GetBytes(
            $$"""{"incarnationId":"{{Guid.Empty}}","sessionId":"{{Guid.Empty}}","status":{{status}},"type":"status"}"""))).Status);
        Assert.Equal(new TerminalStatus("claude", "blocked", "question"), Read("""{"kind":"question","program":"claude","state":"blocked"}"""));
        Assert.Null(Read("null"));
        foreach (var invalid in new[]
                 {
                     """{"state":"blocked","message":"Allow the command?"}""", """{"state":"thinking"}""",
                     """{"state":"working","kind":"permission"}""", """{"program":"Claude Code"}""",
                 })
            Assert.ThrowsAny<Exception>(() => Read(invalid));
    }

    [Fact]
    public void AConnectionMessageWithoutAKnownTypeIsRefused()
    {
        foreach (var message in new[] { "{}", $$"""{"sessionId":"{{Guid.Empty}}"}""", """{"type":"other"}""", "null" })
        {
            var refusal = Record.Exception(() => Wire.Decode(Encoding.UTF8.GetBytes(message)));
            Assert.True(refusal is ApiException or JsonException, $"{message}: {refusal?.GetType().Name}");
        }
    }

    [Fact]
    public async Task TheWatchdogEndsASilentConnectionAndOneThatDoesNotTakeItsMessages()
    {
        var ct = TestContext.Current.CancellationToken;
        var clock = new SteppedClock(); var directory = new ConnectionDirectory(); var stuck = new StuckSocket();
        await using var quiet = new DeviceConnection(new FakeSocket(), Guid.CreateVersion7(), "quiet", directory.Relays, clock);
        await using var talking = new DeviceConnection(new FakeSocket(), Guid.CreateVersion7(), "talking", directory.Relays, clock);
        await using var slow = new DeviceConnection(stuck, Guid.CreateVersion7(), "slow", directory.Relays, clock);
        directory.Register(quiet); directory.Register(talking); directory.Register(slow);
        var watchdog = new ConnectionWatchdog(directory, clock, Microsoft.Extensions.Logging.Abstractions.NullLogger<ConnectionWatchdog>.Instance);
        Assert.True(slow.Send([1]));
        await stuck.Sending.Task.WaitAsync(TimeSpan.FromSeconds(5), ct);

        clock.Advance(ConnectionWatchdog.SendLimit + TimeSpan.FromSeconds(1)); talking.Heard(); slow.Heard();
        await watchdog.RunAsync(ct);
        Assert.True(quiet.IsOpen); Assert.True(talking.IsOpen); Assert.False(slow.IsOpen);

        clock.Advance(ConnectionWatchdog.Silence - ConnectionWatchdog.SendLimit); talking.Heard();
        await watchdog.RunAsync(ct);
        Assert.False(quiet.IsOpen); Assert.True(talking.IsOpen);
    }

    [Fact]
    public async Task AStoppingServerClosesEachConnectionAsARestartAndKeepsNoRoute()
    {
        var directory = new ConnectionDirectory(); var state = Terminal(); var socket = new ClosingSocket();
        await using var host = Connection(socket, Guid.CreateVersion7(), "host", directory.Relays);
        directory.Register(host); directory.Host(host, state);
        directory.StopAll();
        Assert.False(host.IsOpen);
        Assert.False(host.Send([1]));
        var (status, description) = await socket.Closed.Task.WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
        Assert.Equal((WebSocketCloseStatus)1012, status);
        Assert.Equal("The Kodosi server is restarting.", description);
        Assert.Equal((0, 0, 0), directory.Counts());
        Assert.False(IsOnline(directory, state.Id));
    }

    [Fact]
    public async Task ADeviceCallsOnlyAnotherConnectedComputerOfItsPerson()
    {
        var directory = new ConnectionDirectory(); var user = Guid.CreateVersion7();
        var (computer, computerSocket) = Device(directory, user, "computer", hosts: true);
        var (phone, phoneSocket) = Device(directory, user, "phone");
        var (stranger, _) = Device(directory, device: "stranger", hosts: true);
        await using var disposeComputer = computer; await using var disposePhone = phone; await using var disposeStranger = stranger;
        directory.Register(computer); directory.Register(phone); directory.Register(stranger);
        directory.Ready(computer); directory.Ready(stranger);
        Assert.False(phoneSocket.Frames.Reader.TryRead(out _));
        directory.Ready(phone);
        Assert.Equal(["computer"], Hosts(await MessageAsync(phoneSocket, "hosts")));

        var pipe = Guid.CreateVersion7();
        directory.Call(phone, pipe, "computer");
        var caller = await MessageAsync(computerSocket, "caller");
        Assert.Equal(pipe, caller.GetProperty("pipe").GetGuid());
        Assert.Equal(user, caller.GetProperty("userId").GetGuid());
        Assert.Equal("phone", caller.GetProperty("deviceId").GetString());
        var up = BackendApplication.PipeFrame(pipe, [1]); var down = BackendApplication.PipeFrame(pipe, [2]);
        Assert.Equal(up.Length, directory.Relay(phone, pipe, up));
        Assert.Equal(up, await BinaryAsync(computerSocket));
        Assert.Equal(down.Length, directory.Relay(computer, pipe, down));
        Assert.Equal(down, await BinaryAsync(phoneSocket));

        foreach (var (from, to) in new[] { (phone, "stranger"), (phone, "phone"), (computer, "computer"), (computer, "phone"), (phone, "missing") })
            Assert.Equal(409, Assert.Throws<ApiException>(() => directory.Call(from, Guid.CreateVersion7(), to)).Status);
    }

    [Fact]
    public async Task AComputerTakesFewCallsAtOnceAndItsLossEndsThemAndTellsTheOtherDevices()
    {
        var directory = new ConnectionDirectory(); var user = Guid.CreateVersion7();
        var (computer, _) = Device(directory, user, "computer", hosts: true);
        var (laptop, laptopSocket) = Device(directory, user, "laptop", hosts: true);
        var (older, olderSocket) = Device(directory, user, "older", version: ContractVersions.OldestTerminalConnection);
        await using var disposeComputer = computer; await using var disposeLaptop = laptop; await using var disposeOlder = older;
        foreach (var device in new[] { computer, laptop, older }) { directory.Register(device); directory.Ready(device); }
        Assert.Equal(["computer", "laptop"], Hosts(await MessageAsync(laptopSocket, "hosts")));
        var calls = Enumerable.Range(0, ConnectionDirectory.MaximumCalls).Select(_ => Guid.CreateVersion7()).ToArray();
        foreach (var call in calls) directory.Call(laptop, call, "computer");
        Assert.Equal(503, Assert.Throws<ApiException>(() => directory.Call(laptop, Guid.CreateVersion7(), "computer")).Status);
        directory.Close(laptop, calls[0]);
        directory.Call(laptop, Guid.CreateVersion7(), "computer");

        directory.Remove(computer);
        Assert.Equal(["laptop"], Hosts(await MessageAsync(laptopSocket, "hosts")));
        var closed = await MessageAsync(laptopSocket, "closed");
        Assert.Equal(409, closed.GetProperty("status").GetInt32());
        Assert.Equal("That computer is not online.", closed.GetProperty("message").GetString());
        Assert.Equal((2, 0, 0), directory.Counts());
        while (olderSocket.Frames.Reader.TryRead(out var frame))
            Assert.DoesNotContain("\"hosts\"", Encoding.UTF8.GetString(frame.Bytes), StringComparison.Ordinal);
    }

    [Fact]
    public async Task AComputerThatConnectsAgainBeforeItsOldConnectionEndsStaysReachable()
    {
        var directory = new ConnectionDirectory(); var user = Guid.CreateVersion7();
        var (old, _) = Device(directory, user, "computer", hosts: true);
        var (computer, computerSocket) = Device(directory, user, "computer", hosts: true);
        var (laptop, laptopSocket) = Device(directory, user, "laptop");
        await using var disposeOld = old; await using var disposeComputer = computer; await using var disposeLaptop = laptop;
        directory.Register(old); directory.Ready(old);
        directory.Register(computer); directory.Ready(computer);
        Assert.Equal(["computer"], Hosts(await MessageAsync(computerSocket, "hosts")));
        directory.Register(laptop); directory.Ready(laptop);
        Assert.Equal(["computer"], Hosts(await MessageAsync(laptopSocket, "hosts")));
        var pipe = Guid.CreateVersion7();
        directory.Call(laptop, pipe, "computer");
        Assert.Equal(pipe, (await MessageAsync(computerSocket, "caller")).GetProperty("pipe").GetGuid());
    }

    private static string[] Hosts(JsonElement notice) => notice.GetProperty("deviceIds").EnumerateArray().Select(x => x.GetString()!).ToArray();

    private static bool IsOnline(ConnectionDirectory directory, Guid sessionId) => directory.Live(sessionId).Online;

    private sealed class SteppedClock : TimeProvider
    {
        private long ticks = DateTimeOffset.UtcNow.UtcTicks;
        public override long TimestampFrequency => TimeSpan.TicksPerSecond;
        public override long GetTimestamp() => Interlocked.Read(ref ticks);
        public override DateTimeOffset GetUtcNow() => new(Interlocked.Read(ref ticks), TimeSpan.Zero);
        public void Advance(TimeSpan duration) => Interlocked.Add(ref ticks, duration.Ticks);
    }

    private sealed class StuckSocket : FakeSocket
    {
        public TaskCompletionSource Sending { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public override Task SendAsync(ArraySegment<byte> buffer, WebSocketMessageType type, bool endOfMessage, CancellationToken ct)
        { Sending.TrySetResult(); return Task.Delay(Timeout.Infinite, ct); }
    }

    private sealed class ClosingSocket : FakeSocket
    {
        public TaskCompletionSource<(WebSocketCloseStatus Status, string? Description)> Closed { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public override Task CloseOutputAsync(WebSocketCloseStatus closeStatus, string? statusDescription, CancellationToken cancellationToken)
        { Closed.TrySetResult((closeStatus, statusDescription)); return base.CloseOutputAsync(closeStatus, statusDescription, cancellationToken); }
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
