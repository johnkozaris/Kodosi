using System.Collections.Concurrent;

namespace Kodosi.TerminalConnections;

public sealed class ConnectionDirectory
{
    internal const int MaximumDeviceConnections = 20_000;
    internal const int MaximumHostedTerminals = 65_536;
    internal const int MaximumSessionPipes = 64;
    internal const int MaximumDevicePipes = 256;
    internal const int MaximumCalls = 16;
    private readonly Lock sync = new();
    private readonly Dictionary<Guid, Dictionary<string, Entry>> accounts = [];
    private readonly Dictionary<DeviceConnection, Entry> entries = new(ReferenceEqualityComparer.Instance);
    private readonly Dictionary<Guid, Hosted> hosted = [];
    private readonly ConcurrentDictionary<Guid, Pipe> pipes = new();

    internal event Action<Guid>? HostChanged;

    public ConnectionDirectory() : this(RelayBudget.ServerBytes) { }

    internal ConnectionDirectory(long relayBytes) => Relays = new RelayBudget(relayBytes);

    internal RelayBudget Relays { get; }

    public (bool Online, TerminalStatus? Status) Live(Guid sessionId)
    {
        lock (sync) return hosted.TryGetValue(sessionId, out var live) && live.Host.Connection.IsOpen ? (true, live.Status) : (false, null);
    }

    internal (int Devices, int Pipes, int Hosted) Counts()
    {
        lock (sync) return (entries.Count, pipes.Count, hosted.Count);
    }

    internal DeviceConnection[] Online()
    {
        lock (sync) return entries.Keys.Where(x => x.IsOpen).ToArray();
    }

    public void Notify(IEnumerable<Guid> users, string surface, Guid? roomId = null, long? sequence = null) =>
        Send(users, Wire.Encode(new ChangedNotice(surface, roomId, sequence)));

    public void NotifyTerminal(Guid sessionId, Guid incarnationId, Guid ownerId, string hostDeviceId, IEnumerable<Guid> people)
    {
        var (online, status) = Live(sessionId);
        Send(people, Wire.Encode(new TerminalNotice(sessionId, incarnationId, online, status)),
            connection => connection.UserId == ownerId && connection.DeviceId == hostDeviceId);
    }

    private void Send(IEnumerable<Guid> users, byte[] message, Func<DeviceConnection, bool>? except = null)
    {
        List<DeviceConnection> targets = [];
        lock (sync)
            foreach (var user in users.Distinct())
                if (accounts.TryGetValue(user, out var devices))
                    targets.AddRange(devices.Values.Select(x => x.Connection).Where(x => except?.Invoke(x) != true));
        foreach (var connection in targets) connection.Send(message);
    }

    internal void Register(DeviceConnection connection)
    {
        var ending = new Ending();
        lock (sync)
        {
            var devices = accounts.GetValueOrDefault(connection.UserId);
            var previous = devices?.GetValueOrDefault(connection.DeviceId);
            if (previous is null && ((devices?.Count ?? 0) >= Limits.MaxDevicesPerUser || entries.Count >= MaximumDeviceConnections))
                throw new ApiException(503, "Too many device connections.");
            if (previous is not null)
            {
                DetachLocked(previous, ending);
                devices = accounts.GetValueOrDefault(connection.UserId);
            }
            if (devices is null) accounts[connection.UserId] = devices = new Dictionary<string, Entry>(StringComparer.Ordinal);
            var entry = new Entry(connection);
            devices[connection.DeviceId] = entry;
            entries[connection] = entry;
        }
        ending.Apply(this);
    }

    internal void Ready(DeviceConnection connection)
    {
        lock (sync)
        {
            if (!entries.TryGetValue(connection, out var entry)) return;
            entry.Ready = true;
            TellHostsLocked(connection.UserId, connection.Hosts ? null : connection);
        }
    }

    internal void Remove(DeviceConnection connection)
    {
        var ending = new Ending();
        lock (sync)
            if (entries.TryGetValue(connection, out var entry)) DetachLocked(entry, ending);
        ending.Apply(this);
        connection.Abort();
    }

    public void RemoveDevice(Guid userId, string deviceId)
    {
        var ending = new Ending();
        lock (sync)
            if (accounts.GetValueOrDefault(userId)?.GetValueOrDefault(deviceId) is { } entry) DetachLocked(entry, ending);
        ending.Apply(this);
    }

    internal void Host(DeviceConnection connection, Data.Session session)
    {
        var ending = new Ending();
        lock (sync)
        {
            if (!entries.TryGetValue(connection, out var entry) || !connection.IsOpen) throw ApiException.Conflict("The device connection closed.");
            if (hosted.TryGetValue(session.Id, out var live))
            {
                if (live.Host == entry && live.IncarnationId == session.IncarnationId) return;
                EndPipesLocked(live.Pipes, 409, "This terminal was replaced.", ending);
                live.Host.Hosted.Remove(session.Id);
                live.Host = entry; live.IncarnationId = session.IncarnationId; live.Status = null;
            }
            else
            {
                if (hosted.Count >= MaximumHostedTerminals) throw new ApiException(503, "The server has reached its limit of terminal connections.");
                hosted[session.Id] = new Hosted(session.IncarnationId, entry);
            }
            entry.Hosted.Add(session.Id);
            ending.Changed.Add(session.Id);
        }
        ending.Apply(this);
    }

    internal void Share(DeviceConnection connection, Guid sessionId, Guid incarnationId, TerminalStatus? status)
    {
        lock (sync)
        {
            if (!hosted.TryGetValue(sessionId, out var live) || live.Host.Connection != connection || live.IncarnationId != incarnationId
                || live.Status == status) return;
            live.Status = status;
        }
        HostChanged?.Invoke(sessionId);
    }

    internal void Unhost(DeviceConnection connection, Guid sessionId)
    {
        var ending = new Ending();
        lock (sync)
            if (hosted.TryGetValue(sessionId, out var live) && live.Host.Connection == connection) RemoveHostedLocked(sessionId, live, ending);
        ending.Apply(this);
    }

    public void RemoveSession(Guid sessionId)
    {
        var ending = new Ending();
        lock (sync)
            if (hosted.TryGetValue(sessionId, out var live)) RemoveHostedLocked(sessionId, live, ending);
        ending.Apply(this);
    }

    internal void Open(DeviceConnection viewer, Guid pipeId, Data.Session session)
    {
        lock (sync)
        {
            if (!entries.TryGetValue(viewer, out var watcher)) throw ApiException.Conflict("The device connection closed.");
            if (!hosted.TryGetValue(session.Id, out var live) || !live.Host.Connection.IsOpen) throw ApiException.Conflict("The host is offline.");
            if (live.IncarnationId != session.IncarnationId) throw ApiException.Conflict("This terminal was replaced.");
            if (live.Pipes.Count >= MaximumSessionPipes) throw new ApiException(503, "Too many connected participants.");
            if (watcher.Views >= MaximumDevicePipes) throw new ApiException(503, "Too many open views.");
            if (!pipes.TryAdd(pipeId, new Pipe(pipeId, session.Id, viewer, live.Host.Connection))) throw ApiException.Conflict("The view identity is in use.");
            live.Pipes.Add(pipeId);
            watcher.Pipes.Add(pipeId);
            watcher.Views++;
            live.Host.Pipes.Add(pipeId);
            live.Host.Connection.Send(Wire.Encode(new ViewerNotice(pipeId, session.Id, viewer.UserId, viewer.DeviceId)));
        }
    }

    internal void Call(DeviceConnection caller, Guid pipeId, string deviceId)
    {
        lock (sync)
        {
            if (!entries.TryGetValue(caller, out var calling)) throw ApiException.Conflict("The device connection closed.");
            if (accounts.GetValueOrDefault(caller.UserId)?.GetValueOrDefault(deviceId) is not { Ready: true } called || called == calling
                || !called.Connection.Hosts || !called.Connection.IsOpen)
                throw ApiException.Conflict("That computer is not online.");
            if (called.Calls >= MaximumCalls) throw new ApiException(503, "That computer is busy. Try again in a moment.");
            if (calling.Views >= MaximumDevicePipes) throw new ApiException(503, "Too many open views.");
            if (!pipes.TryAdd(pipeId, new Pipe(pipeId, Guid.Empty, caller, called.Connection))) throw ApiException.Conflict("The view identity is in use.");
            calling.Pipes.Add(pipeId);
            calling.Views++;
            called.Pipes.Add(pipeId);
            called.Calls++;
            called.Connection.Send(Wire.Encode(new CallerNotice(pipeId, caller.UserId, caller.DeviceId)));
        }
    }

    internal void Close(DeviceConnection connection, Guid pipeId)
    {
        if (!pipes.TryGetValue(pipeId, out var pipe) || (pipe.Viewer != connection && pipe.Host != connection)) return;
        var ending = new Ending();
        lock (sync)
            if (RemovePipeLocked(pipe)) ending.Pipes.Add((pipe, 0, "", connection));
        ending.Apply(this);
    }

    internal int Relay(DeviceConnection from, Guid pipeId, ReadOnlySpan<byte> frame)
    {
        if (!pipes.TryGetValue(pipeId, out var pipe)) return 0;
        var to = pipe.Viewer == from ? pipe.Host : pipe.Host == from ? pipe.Viewer : null;
        if (to is null) return 0;
        var (status, message) = to.Relay(frame.ToArray()) switch
        {
            Relayed.Sent => (0, ""),
            Relayed.Busy => (503, "The Kodosi server is busy. Open the terminal again in a moment."),
            _ => (503, "The connection is too slow."),
        };
        if (status == 0) return frame.Length;
        var ending = new Ending();
        lock (sync)
            if (RemovePipeLocked(pipe)) ending.Pipes.Add((pipe, status, message, null));
        ending.Apply(this);
        return 0;
    }

    public void Revoke(Guid sessionId, Func<Guid, string, bool> keep)
    {
        var ending = new Ending();
        DeviceConnection host;
        lock (sync)
        {
            if (!hosted.TryGetValue(sessionId, out var live)) return;
            host = live.Host.Connection;
            foreach (var pipe in live.Pipes.Select(id => pipes[id]).Where(x => !keep(x.Viewer.UserId, x.Viewer.DeviceId)).ToArray())
                if (RemovePipeLocked(pipe)) ending.Pipes.Add((pipe, 403, "This terminal is not shared with you.", null));
        }
        ending.Apply(this);
        host.Send(Wire.Encode(new AccessChangedNotice(sessionId)));
    }

    public void TellHost(Guid sessionId)
    {
        DeviceConnection host;
        lock (sync)
        {
            if (!hosted.TryGetValue(sessionId, out var live)) return;
            host = live.Host.Connection;
        }
        host.Send(Wire.Encode(new AccessChangedNotice(sessionId)));
    }

    public void Renew(Guid userId, string deviceId, DateTimeOffset expires)
    {
        lock (sync) accounts.GetValueOrDefault(userId)?.GetValueOrDefault(deviceId)?.Connection.ExtendUntil(expires);
    }

    public void StopAll()
    {
        DeviceConnection[] all;
        lock (sync)
        {
            all = entries.Keys.ToArray();
            accounts.Clear(); entries.Clear(); hosted.Clear(); pipes.Clear();
        }
        foreach (var connection in all) connection.Close();
    }

    private void DetachLocked(Entry entry, Ending ending)
    {
        var connection = entry.Connection;
        foreach (var sessionId in entry.Hosted.ToArray()) RemoveHostedLocked(sessionId, hosted[sessionId], ending, 409, "The host is offline.");
        foreach (var pipe in entry.Pipes.Select(id => pipes[id]).ToArray())
            if (RemovePipeLocked(pipe))
            {
                var lost = pipe.Host == connection;
                ending.Pipes.Add((pipe, lost ? 409 : 0, !lost ? "" : pipe.IsCall ? "That computer is not online." : "The host is offline.", connection));
            }
        entries.Remove(connection);
        if (accounts.TryGetValue(connection.UserId, out var devices) && devices.GetValueOrDefault(connection.DeviceId) == entry)
        {
            devices.Remove(connection.DeviceId);
            if (devices.Count == 0) accounts.Remove(connection.UserId);
        }
        if (entry.Ready && connection.Hosts) TellHostsLocked(connection.UserId);
        ending.Aborted.Add(connection);
    }

    private void TellHostsLocked(Guid userId, DeviceConnection? only = null)
    {
        if (!accounts.TryGetValue(userId, out var devices)) return;
        var ready = devices.Values.Where(x => x.Ready).ToArray();
        var hosts = ready.Where(x => x.Connection.Hosts && x.Connection.IsOpen).Select(x => x.Connection.DeviceId).Order(StringComparer.Ordinal).ToArray();
        var notice = Wire.Encode(new HostsNotice(hosts));
        foreach (var entry in ready.Where(x => only is null || x.Connection == only)) entry.Connection.Send(notice);
    }

    private void RemoveHostedLocked(Guid sessionId, Hosted live, Ending ending, int status = 404, string message = "This terminal is no longer available.")
    {
        hosted.Remove(sessionId);
        live.Host.Hosted.Remove(sessionId);
        EndPipesLocked(live.Pipes, status, message, ending);
        ending.Changed.Add(sessionId);
    }

    private void EndPipesLocked(IEnumerable<Guid> ids, int status, string message, Ending ending)
    {
        foreach (var pipe in ids.Select(id => pipes[id]).ToArray())
            if (RemovePipeLocked(pipe)) ending.Pipes.Add((pipe, status, message, null));
    }

    private bool RemovePipeLocked(Pipe pipe)
    {
        if (!pipes.TryRemove(new KeyValuePair<Guid, Pipe>(pipe.Id, pipe))) return false;
        if (hosted.TryGetValue(pipe.SessionId, out var live)) live.Pipes.Remove(pipe.Id);
        if (entries.TryGetValue(pipe.Viewer, out var viewer))
        {
            viewer.Pipes.Remove(pipe.Id);
            viewer.Views--;
        }
        if (entries.TryGetValue(pipe.Host, out var host))
        {
            host.Pipes.Remove(pipe.Id);
            if (pipe.IsCall) host.Calls--;
        }
        return true;
    }

    private sealed class Entry(DeviceConnection connection)
    {
        public DeviceConnection Connection { get; } = connection;
        public HashSet<Guid> Hosted { get; } = [];
        public HashSet<Guid> Pipes { get; } = [];
        public int Views { get; set; }
        public int Calls { get; set; }
        public bool Ready { get; set; }
    }

    private sealed class Hosted(Guid incarnationId, Entry host)
    {
        public Guid IncarnationId { get; set; } = incarnationId;
        public Entry Host { get; set; } = host;
        public TerminalStatus? Status { get; set; }
        public HashSet<Guid> Pipes { get; } = [];
    }

    // Messages and closes go out after the lock is released, in the order that the lock gave them.
    private sealed class Ending
    {
        public List<(Pipe Pipe, int Status, string Message, DeviceConnection? Silent)> Pipes { get; } = [];
        public List<DeviceConnection> Aborted { get; } = [];
        public List<Guid> Changed { get; } = [];

        public void Apply(ConnectionDirectory directory)
        {
            foreach (var connection in Aborted) connection.Abort();
            foreach (var (pipe, status, message, silent) in Pipes)
            {
                var closed = Wire.Encode(new ClosedNotice(pipe.Id, status, message));
                if (pipe.Viewer != silent) pipe.Viewer.Send(closed);
                if (pipe.Host != silent && pipe.Host != pipe.Viewer) pipe.Host.Send(closed);
            }
            foreach (var sessionId in Changed) directory.HostChanged?.Invoke(sessionId);
        }
    }
}
