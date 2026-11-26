using System.Buffers.Binary;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;

namespace Kodosi.TerminalConnections;

/// <summary>
/// Answers STUN, so devices learn their public addresses from the Kodosi server, and relays packets between the two devices of an
/// allocation. QUIC encrypts the packets end to end; the relay forwards them unchanged.
/// </summary>
internal sealed partial class UdpRelay(RelaySettings settings, RelayAllocations allocations, ServerMetrics metrics, TimeProvider clock,
    ILogger<UdpRelay> logger) : BackgroundService
{
    internal const byte Data = 0x10;
    internal const byte Bind = 0x11;
    internal const byte Bound = 0x12;
    internal const int HeaderBytes = 1 + RelayAllocations.IdBytes;
    internal const int MacBytes = 16;
    internal const int BindBytes = HeaderBytes + 1 + sizeof(long) + MacBytes;
    internal const int BoundBytes = HeaderBytes + 1;
    private const byte StunRequest = 0x00;
    private const int StunHeaderBytes = 20;
    private const uint StunCookie = 0x2112A442;
    private const int Readers = 4;
    private const int LargestPacket = 64 * 1024;
    private const int SocketBufferBytes = 8 * 1024 * 1024;
    /// <summary>STUN answers for each source address; a device asks about twice a minute.</summary>
    private const double StunEachSecond = 2;
    private const double StunBurst = 10;
    private static readonly TimeSpan BindSkew = TimeSpan.FromMinutes(1);
    private static readonly IPEndPoint AnyEndPoint = new(IPAddress.IPv6Any, 0);
    private readonly SourceLimit stun = new(StunBurst, StunEachSecond, clock);

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        if (settings.Address is null) return;
        using var socket = Open(settings.Port);
        Listening(logger, settings.Port);
        await RunAsync(socket, stoppingToken);
    }

    internal static Socket Open(int port)
    {
        var socket = new Socket(AddressFamily.InterNetworkV6, SocketType.Dgram, ProtocolType.Udp)
        {
            DualMode = true,
            ReceiveBufferSize = SocketBufferBytes,
            SendBufferSize = SocketBufferBytes,
        };
        socket.Bind(new IPEndPoint(IPAddress.IPv6Any, port));
        return socket;
    }

    internal Task RunAsync(Socket socket, CancellationToken ct) =>
        Task.WhenAll(Enumerable.Range(0, Readers).Select(_ => Task.Run(() => ReadAsync(socket, ct), ct)));

    private async Task ReadAsync(Socket socket, CancellationToken ct)
    {
        var buffer = GC.AllocateUninitializedArray<byte>(LargestPacket, pinned: true);
        var from = new SocketAddress(AddressFamily.InterNetworkV6);
        while (!ct.IsCancellationRequested)
        {
            int length;
            try
            {
                length = await socket.ReceiveFromAsync(buffer, SocketFlags.None, from, ct);
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (SocketException)
            {
                // Some systems report an earlier packet's ICMP error on the next receive.
                continue;
            }
            Answer(socket, buffer.AsSpan(0, length), from);
        }
    }

    private void Answer(Socket socket, Span<byte> packet, SocketAddress from)
    {
        if (packet.IsEmpty) return;
        switch (packet[0])
        {
            case Data:
                Forward(socket, packet, from);
                break;
            case Bind:
                TakeBind(socket, packet, from);
                break;
            case StunRequest when stun.TryTake(from):
                AnswerStun(socket, packet, from);
                break;
        }
    }

    private void Forward(Socket socket, ReadOnlySpan<byte> packet, SocketAddress from)
    {
        if (packet.Length <= HeaderBytes || allocations.Find(Id(packet)) is not { } allocation) return;
        if (allocation.Peer(from, packet.Length) is not { } to || !Send(socket, packet, to)) return;
        metrics.Relayed(packet.Length - HeaderBytes);
    }

    /// <summary>Binds a slot of an allocation to the address that a bind signed with its secret came from, and confirms it there.</summary>
    private void TakeBind(Socket socket, Span<byte> packet, SocketAddress from)
    {
        if (packet.Length != BindBytes || allocations.Find(Id(packet)) is not { } allocation || !allocation.MayBind()) return;
        var slot = packet[HeaderBytes];
        if (slot > 1) return;
        var time = BinaryPrimitives.ReadInt64BigEndian(packet[(HeaderBytes + 1)..]);
        var now = clock.GetUtcNow().ToUnixTimeMilliseconds();
        var skew = (long)BindSkew.TotalMilliseconds;
        if (time < now - skew || time > now + skew || !Signed(packet, allocation.Secrets[slot]) || !allocation.Bind(slot, time, from)) return;
        packet[0] = Bound;
        Send(socket, packet[..BoundBytes], from);
    }

    /// <summary>Whether a bind carries the signature of the slot's secret.</summary>
    internal static bool Signed(ReadOnlySpan<byte> bind, byte[] secret)
    {
        Span<byte> mac = stackalloc byte[HMACSHA256.HashSizeInBytes];
        HMACSHA256.HashData(secret, bind[..^MacBytes], mac);
        return CryptographicOperations.FixedTimeEquals(mac[..MacBytes], bind[^MacBytes..]);
    }

    /// <summary>Answers an RFC 8489 Binding request with the address it came from.</summary>
    private static void AnswerStun(Socket socket, ReadOnlySpan<byte> packet, SocketAddress from)
    {
        if (packet.Length < StunHeaderBytes || packet[1] != 0x01 || BinaryPrimitives.ReadUInt32BigEndian(packet[4..]) != StunCookie
            || BinaryPrimitives.ReadUInt16BigEndian(packet[2..]) != packet.Length - StunHeaderBytes) return;
        var seen = (IPEndPoint)AnyEndPoint.Create(from);
        var address = seen.Address.IsIPv4MappedToIPv6 ? seen.Address.MapToIPv4() : seen.Address;
        var addressBytes = address.AddressFamily == AddressFamily.InterNetwork ? 4 : 16;
        var attributeBytes = 4 + 4 + addressBytes;
        Span<byte> answer = stackalloc byte[StunHeaderBytes + 4 + 4 + 16];
        BinaryPrimitives.WriteUInt16BigEndian(answer, 0x0101);
        BinaryPrimitives.WriteUInt16BigEndian(answer[2..], (ushort)attributeBytes);
        packet[4..StunHeaderBytes].CopyTo(answer[4..]);
        BinaryPrimitives.WriteUInt16BigEndian(answer[20..], 0x0020);
        BinaryPrimitives.WriteUInt16BigEndian(answer[22..], (ushort)(attributeBytes - 4));
        answer[24] = 0;
        answer[25] = addressBytes == 4 ? (byte)0x01 : (byte)0x02;
        BinaryPrimitives.WriteUInt16BigEndian(answer[26..], (ushort)(seen.Port ^ (StunCookie >> 16)));
        address.TryWriteBytes(answer[28..], out _);
        for (var at = 0; at < addressBytes; at++) answer[28 + at] ^= packet[4 + at];
        Send(socket, answer[..(StunHeaderBytes + attributeBytes)], from);
    }

    internal static ulong Id(ReadOnlySpan<byte> packet)
    {
        Span<byte> id = stackalloc byte[sizeof(ulong)];
        id[0] = 0;
        packet.Slice(1, RelayAllocations.IdBytes).CopyTo(id[1..]);
        return BinaryPrimitives.ReadUInt64BigEndian(id);
    }

    private static bool Send(Socket socket, ReadOnlySpan<byte> packet, SocketAddress to)
    {
        try
        {
            socket.SendTo(packet, SocketFlags.None, to);
            return true;
        }
        catch (SocketException)
        {
            return false;
        }
    }

    [LoggerMessage(Level = LogLevel.Information, Message = "The UDP relay and STUN listen on port {Port}")]
    private static partial void Listening(ILogger logger, int port);
}
