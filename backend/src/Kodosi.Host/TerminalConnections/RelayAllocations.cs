using System.Collections.Concurrent;
using System.Net;
using System.Security.Cryptography;

namespace Kodosi.TerminalConnections;

/// <summary>Where devices reach the UDP relay, which also answers STUN. Without an address the server relays only over WebSockets.</summary>
internal sealed record RelaySettings(string? Address, int Port)
{
    internal const int DefaultPort = 3478;

    public RelayPlace? Place => Address is null ? null : new RelayPlace(Address, Port);

    public static RelaySettings From(IConfiguration configuration)
    {
        var address = configuration["Relay:Address"];
        var port = configuration.GetValue("Relay:Port", DefaultPort);
        if (port is < 1 or > ushort.MaxValue) throw new InvalidOperationException("Relay:Port must be a UDP port.");
        if (string.IsNullOrWhiteSpace(address)) return new RelaySettings(null, port);
        if (Uri.CheckHostName(address) == UriHostNameType.Unknown) throw new InvalidOperationException("Relay:Address must be a host name or an IP address.");
        return new RelaySettings(address, port);
    }
}

/// <summary>The two slots of a pair of devices, between which the relay forwards packets once each slot is bound to the address its device sends from.</summary>
internal sealed class Allocation
{
    internal const int SecretBytes = 32;
    /// <summary>A pair may relay 80 Mbit/s, with bursts of a few frames of 4K video.</summary>
    private const double BytesEachSecond = 10_000_000;
    private const double BurstBytes = 2_000_000;
    private readonly Lock sync = new();
    private readonly SocketAddress?[] addresses = new SocketAddress?[2];
    private readonly long[] boundAt = new long[2];
    private TokenBucket bandwidth;

    public Allocation(ulong id, TimeProvider clock)
    {
        Id = id;
        Secrets = [RandomNumberGenerator.GetBytes(SecretBytes), RandomNumberGenerator.GetBytes(SecretBytes)];
        bandwidth = new TokenBucket(BurstBytes, BytesEachSecond, clock);
    }

    public ulong Id { get; }

    /// <summary>The key with which the device in each slot signs its binds.</summary>
    public byte[][] Secrets { get; }

    /// <summary>Binds a slot to the address that its bind came from, unless the slot took a bind as late before, which stops replays.</summary>
    public bool Bind(int slot, long time, SocketAddress from)
    {
        lock (sync)
        {
            if (time <= boundAt[slot]) return false;
            boundAt[slot] = time;
            var copy = new SocketAddress(from.Family, from.Size);
            from.Buffer.Span[..from.Size].CopyTo(copy.Buffer.Span);
            addresses[slot] = copy;
            return true;
        }
    }

    /// <summary>Where a packet from a bound slot goes: the other slot, while it is bound and the pair has the bandwidth.</summary>
    public SocketAddress? Peer(SocketAddress from, int bytes)
    {
        lock (sync)
        {
            var to = from.Equals(addresses[0]) ? addresses[1] : from.Equals(addresses[1]) ? addresses[0] : null;
            return to is not null && bandwidth.TryTake(bytes) ? to : null;
        }
    }
}

internal sealed class RelayAllocations(TimeProvider clock)
{
    internal const int IdBytes = 7;
    private const ulong Ids = 1UL << (8 * IdBytes);
    private readonly ConcurrentDictionary<ulong, Allocation> allocations = new();

    public int Count => allocations.Count;

    public Allocation Open()
    {
        while (true)
        {
            var id = BitConverter.ToUInt64(RandomNumberGenerator.GetBytes(sizeof(ulong))) % Ids;
            var allocation = new Allocation(id, clock);
            if (id != 0 && allocations.TryAdd(id, allocation)) return allocation;
        }
    }

    public Allocation? Find(ulong id) => allocations.GetValueOrDefault(id);

    public void Close(ulong id) => allocations.TryRemove(id, out _);

    public void Clear() => allocations.Clear();
}
