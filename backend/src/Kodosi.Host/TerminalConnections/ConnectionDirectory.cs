using System.Collections.Concurrent;
using Kodosi.Data;

namespace Kodosi.TerminalConnections;

public sealed class ConnectionDirectory
{
    private const int MaximumSessions = 4096;
    private const int MaximumSessionPipes = 64;
    private const int MaximumDevicePipes = 256;
    private const int MaximumUserDevices = 16;
    private readonly object sync = new();
    private readonly Dictionary<(Guid User, string Device), DeviceLink> devices = new();
    private readonly Dictionary<Guid, Hosted> hosted = new();
    private readonly ConcurrentDictionary<Guid, Pipe> pipes = new();

    internal event Action<Guid>? HostChanged;

    public ConnectionDirectory() : this(RelayBudget.ServerBytes) { }

    internal ConnectionDirectory(long relayBytes) => Relays = new RelayBudget(relayBytes);

    internal RelayBudget Relays { get; }

    public bool HostOnline(Guid id) { lock (sync) return hosted.TryGetValue(id, out var live) && live.Host.IsOpen; }

    public TerminalStatus? Status(Guid id) { lock (sync) return hosted.TryGetValue(id, out var live) && live.Host.IsOpen ? live.Status : null; }

    internal (int Devices, int Pipes) Count() { lock (sync) return (devices.Count, pipes.Count); }

    internal DeviceLink[] Online() { lock (sync) return devices.Values.Where(link => link.IsOpen).ToArray(); }

    public void Notify(Guid userId, string surface) => Send(userId, new { type = "changed", surface });

    public void Notify(Guid userId, string surface, Guid roomId) => Send(userId, new { type = "changed", surface, roomId });

    public void NotifyContent(Guid userId, Guid roomId, long sequence) =>
        Send(userId, new { type = "changed", surface = "rooms", roomId, sequence });

    public void NotifyTerminal(Session session, IReadOnlySet<Guid> audience)
    {
        var message = new
        {
            type = "terminal", sessionId = session.Id, incarnationId = session.IncarnationId,
            online = HostOnline(session.Id), status = Status(session.Id)
        };
        foreach (var link in Online().Where(link => audience.Contains(link.UserId)
                     && !(link.UserId == session.OwnerUserId && link.DeviceId == session.HostDeviceId)))
            link.Send(message);
    }

    private void Send(Guid userId, object message)
    {
        foreach (var link in Online().Where(link => link.UserId == userId)) link.Send(message);
    }

    internal void Register(DeviceLink link)
    {
        DeviceLink? previous;
        lock (sync)
        {
            var key = (link.UserId, link.DeviceId);
            if (!devices.TryGetValue(key, out previous) && devices.Keys.Count(x => x.User == link.UserId) >= MaximumUserDevices)
                throw new ApiException(503, "Too many device connections.");
            devices[key] = link;
        }
        if (previous is not null) Detach(previous);
    }

    internal void Remove(DeviceLink link)
    {
        lock (sync)
        {
            var key = (link.UserId, link.DeviceId);
            if (devices.TryGetValue(key, out var current) && ReferenceEquals(current, link)) devices.Remove(key);
        }
        Detach(link);
    }

    private void Detach(DeviceLink link)
    {
        link.Abort();
        Pipe[] ended;
        Guid[] left;
        lock (sync)
        {
            left = hosted.Where(x => ReferenceEquals(x.Value.Host, link)).Select(x => x.Key).ToArray();
            foreach (var id in left) hosted.Remove(id);
            ended = pipes.Values.Where(pipe => ReferenceEquals(pipe.Host, link) || ReferenceEquals(pipe.Viewer, link)).ToArray();
        }
        foreach (var pipe in ended) End(pipe, ReferenceEquals(pipe.Host, link) ? 409 : 0, ReferenceEquals(pipe.Host, link) ? "The host is offline." : "");
        foreach (var id in left) HostChanged?.Invoke(id);
    }

    internal void Host(DeviceLink link, Session state)
    {
        Pipe[] ended = [];
        lock (sync)
        {
            if (!link.IsOpen) throw new ApiException(409, "The device connection closed.");
            if (hosted.TryGetValue(state.Id, out var previous))
            {
                if (ReferenceEquals(previous.Host, link) && previous.IncarnationId == state.IncarnationId) return;
                ended = pipes.Values.Where(pipe => pipe.SessionId == state.Id).ToArray();
            }
            else if (hosted.Count >= MaximumSessions) throw new ApiException(503, "Terminal connection capacity reached.");
            hosted[state.Id] = new Hosted(state.IncarnationId, link);
        }
        foreach (var pipe in ended) End(pipe, 409, "Session publication changed.");
        HostChanged?.Invoke(state.Id);
    }

    internal void Share(DeviceLink link, Guid sessionId, Guid incarnationId, TerminalStatus? status)
    {
        lock (sync)
        {
            if (!hosted.TryGetValue(sessionId, out var live) || !ReferenceEquals(live.Host, link) || live.IncarnationId != incarnationId
                || live.Status == status) return;
            hosted[sessionId] = live with { Status = status };
        }
        HostChanged?.Invoke(sessionId);
    }

    internal void Unhost(DeviceLink link, Guid sessionId)
    {
        lock (sync)
        {
            if (!hosted.TryGetValue(sessionId, out var live) || !ReferenceEquals(live.Host, link)) return;
        }
        RemoveSession(sessionId);
    }

    internal void Open(DeviceLink viewer, Guid pipeId, Session state)
    {
        lock (sync)
        {
            if (!hosted.TryGetValue(state.Id, out var live) || !live.Host.IsOpen) throw new ApiException(409, "The host is offline.");
            if (live.IncarnationId != state.IncarnationId) throw ApiException.Conflict("Session publication changed.");
            if (pipes.Values.Count(pipe => pipe.SessionId == state.Id) >= MaximumSessionPipes) throw new ApiException(503, "Too many connected participants.");
            if (pipes.Values.Count(pipe => ReferenceEquals(pipe.Viewer, viewer)) >= MaximumDevicePipes) throw new ApiException(503, "Too many open views.");
            if (!pipes.TryAdd(pipeId, new Pipe(pipeId, state.Id, viewer, live.Host))) throw ApiException.Conflict("The view identity is in use.");
            live.Host.Send(new { type = "viewer", pipe = pipeId, sessionId = state.Id, userId = viewer.UserId, deviceId = viewer.DeviceId });
        }
    }

    internal void Close(DeviceLink link, Guid pipeId)
    {
        if (pipes.TryGetValue(pipeId, out var pipe) && (ReferenceEquals(pipe.Viewer, link) || ReferenceEquals(pipe.Host, link))) End(pipe, 0, "", link);
    }

    internal int Relay(DeviceLink from, Guid pipeId, byte[] frame)
    {
        if (!pipes.TryGetValue(pipeId, out var pipe)) return 0;
        var to = ReferenceEquals(pipe.Viewer, from) ? pipe.Host : ReferenceEquals(pipe.Host, from) ? pipe.Viewer : null;
        if (to is null) return 0;
        switch (to.Relay(frame))
        {
            case Relayed.Sent: return frame.Length;
            case Relayed.Busy: End(pipe, 503, "The Kodosi server is busy. Open the terminal again in a moment."); return 0;
            default: End(pipe, 503, "The connection is too slow."); return 0;
        }
    }

    private void End(Pipe pipe, int status, string message, DeviceLink? silent = null)
    {
        if (!pipes.TryRemove(new KeyValuePair<Guid, Pipe>(pipe.Id, pipe))) return;
        foreach (var link in new[] { pipe.Viewer, pipe.Host })
            if (!ReferenceEquals(link, silent)) link.Send(new { type = "closed", pipe = pipe.Id, status, message });
    }

    public void Revoke(Session state, Func<Guid, string, bool> keep)
    {
        DeviceLink? host;
        lock (sync) host = hosted.TryGetValue(state.Id, out var live) ? live.Host : null;
        foreach (var pipe in pipes.Values.Where(pipe => pipe.SessionId == state.Id && !keep(pipe.Viewer.UserId, pipe.Viewer.DeviceId)).ToArray())
            End(pipe, 403, "This terminal is not shared with you.");
        host?.Send(new { type = "accessChanged", sessionId = state.Id });
    }

    public void Renew(Guid userId, string deviceId, DateTimeOffset expires)
    {
        lock (sync) if (devices.TryGetValue((userId, deviceId), out var link)) link.ExtendUntil(expires);
    }

    public void RemoveDevice(Guid userId, string deviceId)
    {
        DeviceLink? link;
        lock (sync) devices.Remove((userId, deviceId), out link);
        if (link is not null) Detach(link);
    }

    public void RemoveSession(Guid id)
    {
        Pipe[] ended;
        lock (sync)
        {
            if (!hosted.Remove(id)) return;
            ended = pipes.Values.Where(pipe => pipe.SessionId == id).ToArray();
        }
        foreach (var pipe in ended) End(pipe, 404, "This terminal is no longer available.");
        HostChanged?.Invoke(id);
    }

    public void StopAll()
    {
        DeviceLink[] all;
        lock (sync) { all = devices.Values.ToArray(); devices.Clear(); hosted.Clear(); }
        pipes.Clear();
        foreach (var link in all) link.Abort();
    }

    private sealed record Hosted(Guid IncarnationId, DeviceLink Host, TerminalStatus? Status = null);
}
