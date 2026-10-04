using System.Collections.Concurrent;
using Kodosi.Data;

namespace Kodosi.TerminalConnections;

public sealed class ConnectionDirectory
{
    private const int MaximumSessions = 4096;
    private const int MaximumPipes = 64;
    private readonly ConcurrentDictionary<Guid, LiveSession> sessions = new();
    private readonly ConcurrentDictionary<string, SocketPeer> listeners = new(StringComparer.Ordinal);

    public bool HostOnline(Guid id) => sessions.TryGetValue(id, out var live) && live.Host.IsOpen;

    internal (int Devices, int Pipes) Count()
    {
        var pipes = 0;
        foreach (var live in sessions.Values) lock (live.Sync) pipes += live.Pipes.Count;
        return (listeners.Count, pipes);
    }

    internal Peer[] OnlinePeers()
    {
        var peers = new HashSet<Peer>(listeners.Values);
        foreach (var live in sessions.Values)
            lock (live.Sync)
            {
                peers.Add(live.Host);
                foreach (var pipe in live.Pipes.Values)
                {
                    peers.Add(pipe.Viewer);
                    if (pipe.Host is { } host) peers.Add(host);
                }
            }
        return peers.Where(peer => peer.IsOpen).ToArray();
    }

    public void Notify(Guid userId, string surface)
    {
        foreach (var peer in listeners.Values.Where(x => x.UserId == userId))
            peer.Send(new { type = "changed", surface });
    }
    internal bool RegisterEvents(SocketPeer peer)
    {
        if (listeners.Values.Count(x => x.UserId == peer.UserId) >= 16) return false;
        return listeners.TryAdd(peer.ConnectionId, peer);
    }
    internal void RemoveEvents(SocketPeer peer) => listeners.TryRemove(peer.ConnectionId, out _);

    public void Revoke(Session state, Func<Guid, string, bool> keep)
    {
        if (!sessions.TryGetValue(state.Id, out var live)) return;
        lock (live.Sync)
        {
            foreach (var pipe in live.Pipes.Values.Where(pipe => !keep(pipe.Viewer.UserId, pipe.Viewer.DeviceId)).ToArray())
            { pipe.Abort(); live.Pipes.Remove(pipe.Id); }
            live.Host.Send(new { type = "accessChanged" });
        }
    }

    public void Renew(Guid userId, string deviceId, DateTimeOffset expires)
    {
        foreach (var peer in OnlinePeers().Where(x => x.UserId == userId && x.DeviceId == deviceId)) peer.ExtendUntil(expires);
    }

    public void RemoveDevice(Guid userId, string deviceId)
    {
        foreach (var peer in listeners.Values.Where(x => x.UserId == userId && x.DeviceId == deviceId)) peer.Abort();
        foreach (var live in sessions.Values)
            lock (live.Sync)
            {
                if (live.Host.UserId == userId && live.Host.DeviceId == deviceId) live.Host.Abort();
                foreach (var pipe in live.Pipes.Values.Where(pipe => pipe.Viewer.UserId == userId && pipe.Viewer.DeviceId == deviceId).ToArray())
                { pipe.Abort(); live.Pipes.Remove(pipe.Id); }
            }
    }

    public void RemoveSession(Guid id)
    {
        if (sessions.TryRemove(id, out var live)) Close(live);
    }

    public void StopAll()
    {
        foreach (var id in sessions.Keys) RemoveSession(id);
        foreach (var peer in listeners.Values) peer.Abort();
        listeners.Clear();
    }

    internal LiveSession RegisterHost(Session state, SocketPeer host)
    {
        if (sessions.Count >= MaximumSessions && !sessions.ContainsKey(state.Id)) throw new ApiException(503, "Terminal connection capacity reached.");
        var live = new LiveSession(state.Id, state.IncarnationId, host);
        while (true)
        {
            if (sessions.TryGetValue(state.Id, out var previous))
            {
                if (previous.IncarnationId != state.IncarnationId) throw ApiException.Conflict("Session publication changed.");
                if (!sessions.TryUpdate(state.Id, live, previous)) continue;
                Close(previous);
                return live;
            }
            if (sessions.TryAdd(state.Id, live)) return live;
        }
    }

    internal void RemoveHost(LiveSession live)
    {
        sessions.TryRemove(new KeyValuePair<Guid, LiveSession>(live.Id, live));
        Close(live);
    }

    internal (LiveSession Live, Pipe Pipe) OpenPipe(Session state, Peer viewer)
    {
        if (!sessions.TryGetValue(state.Id, out var live) || !live.Host.IsOpen) throw new ApiException(409, "The host is offline.");
        var pipe = new Pipe(Guid.CreateVersion7(), viewer);
        lock (live.Sync)
        {
            if (live.Closed || live.IncarnationId != state.IncarnationId) throw ApiException.Conflict("Session publication changed.");
            if (live.Pipes.Count >= MaximumPipes) throw new ApiException(503, "Too many connected participants.");
            live.Pipes.Add(pipe.Id, pipe);
            live.Host.Send(new { type = "viewer", channelId = pipe.Id, userId = viewer.UserId, deviceId = viewer.DeviceId });
        }
        return (live, pipe);
    }

    internal (LiveSession Live, Pipe Pipe) JoinPipe(Session state, Guid channelId, Peer host)
    {
        if (!sessions.TryGetValue(state.Id, out var live)) throw new ApiException(409, "The host is offline.");
        lock (live.Sync)
        {
            if (live.Closed || live.IncarnationId != state.IncarnationId || live.Host.UserId != host.UserId || live.Host.DeviceId != host.DeviceId
                || !live.Pipes.TryGetValue(channelId, out var pipe) || pipe.Host is not null || !pipe.Viewer.IsOpen)
                throw ApiException.Missing();
            pipe.Host = host;
            return (live, pipe);
        }
    }

    internal static void ClosePipe(LiveSession live, Pipe pipe)
    {
        lock (live.Sync) live.Pipes.Remove(pipe.Id);
        pipe.Abort();
    }

    private static void Close(LiveSession live)
    {
        lock (live.Sync)
        {
            live.Closed = true;
            live.Host.Abort();
            foreach (var pipe in live.Pipes.Values) pipe.Abort();
            live.Pipes.Clear();
        }
    }

    internal sealed class LiveSession(Guid id, Guid incarnationId, SocketPeer host)
    {
        public object Sync { get; } = new();
        public Guid Id { get; } = id;
        public Guid IncarnationId { get; } = incarnationId;
        public SocketPeer Host { get; } = host;
        public bool Closed;
        public Dictionary<Guid, Pipe> Pipes { get; } = new();
    }
}
