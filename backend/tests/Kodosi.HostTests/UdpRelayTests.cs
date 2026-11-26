using System.Buffers.Binary;
using System.Diagnostics.Metrics;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Security;
using Kodosi.TerminalConnections;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;
using Xunit;

namespace Kodosi.HostTests;

public sealed class UdpRelayTests : IAsyncDisposable
{
    private static readonly TimeSpan Wait = TimeSpan.FromSeconds(5);
    private static readonly TimeSpan Silence = TimeSpan.FromMilliseconds(300);
    private static readonly TimeSpan Resend = TimeSpan.FromSeconds(1);
    private readonly CancellationTokenSource stop = new();
    private readonly Socket server = UdpRelay.Open(0);
    private readonly RelayAllocations allocations = new(TimeProvider.System);
    private readonly Task running;

    public UdpRelayTests()
    {
        var meters = new ServiceCollection().AddMetrics().BuildServiceProvider().GetRequiredService<IMeterFactory>();
        var metrics = new ServerMetrics(meters, new ConnectionDirectory(TimeProvider.System), new DeviceSessions(TimeProvider.System));
        var relay = new UdpRelay(new RelaySettings("127.0.0.1", 0), allocations, metrics, TimeProvider.System, NullLogger<UdpRelay>.Instance);
        running = relay.RunAsync(server, stop.Token);
    }

    private IPEndPoint Relay => new(IPAddress.Loopback, ((IPEndPoint)server.LocalEndPoint!).Port);

    [Fact]
    public async Task ADeviceLearnsItsPublicAddressFromTheServer()
    {
        using var device = Device();
        var request = new byte[20];
        request[1] = 0x01;
        BinaryPrimitives.WriteUInt32BigEndian(request.AsSpan(4), 0x2112A442);
        RandomNumberGenerator.Fill(request.AsSpan(8));
        await device.SendToAsync(request, Relay);
        var answer = await ReceiveAsync(device, Wait);
        Assert.NotNull(answer);
        Assert.Equal([0x01, 0x01], answer[..2]);
        Assert.Equal(request[4..], answer[4..20]);
        Assert.Equal(0x0020, BinaryPrimitives.ReadUInt16BigEndian(answer.AsSpan(20)));
        Assert.Equal(0x01, answer[25]);
        var port = BinaryPrimitives.ReadUInt16BigEndian(answer.AsSpan(26)) ^ 0x2112;
        var address = new IPAddress(answer[28..32].Select((octet, at) => (byte)(octet ^ request[4 + at])).ToArray());
        Assert.Equal(device.LocalEndPoint, new IPEndPoint(address, port));
    }

    [Fact]
    public async Task TheRelayForwardsOnlyBetweenTheTwoBoundSlotsAndOnlyWhileTheAllocationLasts()
    {
        var allocation = allocations.Open();
        using var first = Device(); using var second = Device(); using var stranger = Device();
        await BindAsync(first, allocation, 0);
        await BindAsync(second, allocation, 1);
        await first.SendToAsync(Data(allocation, "hello"), Relay);
        Assert.Equal(Data(allocation, "hello"), await ReceiveAsync(second, Wait));
        await second.SendToAsync(Data(allocation, "back"), Relay);
        Assert.Equal(Data(allocation, "back"), await ReceiveAsync(first, Wait));
        await stranger.SendToAsync(Data(allocation, "in between"), Relay);
        Assert.Null(await ReceiveAsync(second, Silence));
        allocations.Close(allocation.Id);
        await first.SendToAsync(Data(allocation, "late"), Relay);
        Assert.Null(await ReceiveAsync(second, Silence));
    }

    [Fact]
    public async Task ABindTakesASlotOnlyWithTheSlotsSecretAFreshTimeAndNoReplay()
    {
        var allocation = allocations.Open();
        using var device = Device(); using var stranger = Device(); using var peer = Device();
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        await stranger.SendToAsync(Bind(allocation, 0, now, allocation.Secrets[1]), Relay);
        await stranger.SendToAsync(Bind(allocation, 0, now - (long)TimeSpan.FromMinutes(2).TotalMilliseconds), Relay);
        Assert.Null(await ReceiveAsync(stranger, Silence));
        var bind = Bind(allocation, 0, now);
        await device.SendToAsync(bind, Relay);
        var bound = await ReceiveAsync(device, Wait);
        Assert.Equal((byte[])[UdpRelay.Bound, .. bind[1..UdpRelay.BoundBytes]], bound);
        await stranger.SendToAsync(bind, Relay);
        Assert.Null(await ReceiveAsync(stranger, Silence));
        await BindAsync(peer, allocation, 1);
        await peer.SendToAsync(Data(allocation, "still yours"), Relay);
        Assert.Equal(Data(allocation, "still yours"), await ReceiveAsync(device, Wait));
    }

    [Fact]
    public async Task AFloodFromOneAddressLeavesBindsAndStunToEveryoneElse()
    {
        var allocation = allocations.Open();
        using var flood = Device(); using var device = Device();
        var request = new byte[20];
        request[1] = 0x01;
        BinaryPrimitives.WriteUInt32BigEndian(request.AsSpan(4), 0x2112A442);
        for (var sent = 0; sent < 20_000; sent++)
        {
            await flood.SendToAsync(new[] { UdpRelay.Bind }, Relay);
            await flood.SendToAsync(request, Relay);
        }
        var bound = await AskAsync(device, () => Bind(allocation, 0, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()), Relay);
        Assert.Equal(UdpRelay.Bound, bound?[0]);
        using var asker = new Socket(AddressFamily.InterNetworkV6, SocketType.Dgram, ProtocolType.Udp);
        asker.Bind(new IPEndPoint(IPAddress.IPv6Loopback, 0));
        var answer = await AskAsync(asker, () => request, new IPEndPoint(IPAddress.IPv6Loopback, Relay.Port));
        Assert.Equal([0x01, 0x01], answer?[..2]);
    }

    public async ValueTask DisposeAsync()
    {
        await stop.CancelAsync();
        await running;
        server.Dispose();
        stop.Dispose();
    }

    private async Task BindAsync(Socket device, Allocation allocation, int slot)
    {
        await device.SendToAsync(Bind(allocation, slot, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()), Relay);
        Assert.Equal(UdpRelay.Bound, (await ReceiveAsync(device, Wait))?[0]);
    }

    /// Asks until an answer comes, once a second as devices do: Linux caps the relay's socket buffer,
    /// so a flood fills it and the system drops the next packet before the relay reads it.
    private static async Task<byte[]?> AskAsync(Socket socket, Func<byte[]> packet, IPEndPoint relay)
    {
        var until = DateTimeOffset.UtcNow + Wait;
        while (DateTimeOffset.UtcNow < until)
        {
            await socket.SendToAsync(packet(), relay);
            if (await ReceiveAsync(socket, Resend) is { } answer)
            {
                return answer;
            }
        }
        return null;
    }

    private static Socket Device()
    {
        var socket = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
        socket.Bind(new IPEndPoint(IPAddress.Loopback, 0));
        return socket;
    }

    private static async Task<byte[]?> ReceiveAsync(Socket socket, TimeSpan wait)
    {
        var buffer = new byte[2048];
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(TestContext.Current.CancellationToken);
        timeout.CancelAfter(wait);
        try
        {
            var any = socket.AddressFamily == AddressFamily.InterNetworkV6 ? IPAddress.IPv6Any : IPAddress.Any;
            var received = await socket.ReceiveFromAsync(buffer, SocketFlags.None, new IPEndPoint(any, 0), timeout.Token);
            return buffer[..received.ReceivedBytes];
        }
        catch (OperationCanceledException) when (!TestContext.Current.CancellationToken.IsCancellationRequested)
        {
            return null;
        }
    }

    private static byte[] Header(byte kind, Allocation allocation, int length)
    {
        var packet = new byte[length];
        packet[0] = kind;
        Span<byte> id = stackalloc byte[sizeof(ulong)];
        BinaryPrimitives.WriteUInt64BigEndian(id, allocation.Id);
        id[1..].CopyTo(packet.AsSpan(1));
        return packet;
    }

    private static byte[] Data(Allocation allocation, string text)
    {
        var packet = Header(UdpRelay.Data, allocation, UdpRelay.HeaderBytes + Encoding.UTF8.GetByteCount(text));
        Encoding.UTF8.GetBytes(text, packet.AsSpan(UdpRelay.HeaderBytes));
        return packet;
    }

    private static byte[] Bind(Allocation allocation, int slot, long time, byte[]? secret = null)
    {
        var packet = Header(UdpRelay.Bind, allocation, UdpRelay.BindBytes);
        packet[UdpRelay.HeaderBytes] = (byte)slot;
        BinaryPrimitives.WriteInt64BigEndian(packet.AsSpan(UdpRelay.HeaderBytes + 1), time);
        HMACSHA256.HashData(secret ?? allocation.Secrets[slot], packet.AsSpan(..^UdpRelay.MacBytes)).AsSpan(..UdpRelay.MacBytes)
            .CopyTo(packet.AsSpan(^UdpRelay.MacBytes..));
        return packet;
    }
}
