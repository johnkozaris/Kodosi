using System.Collections.Concurrent;
using System.Text.Json;
using Kodosi.Data;

namespace Kodosi.TerminalConnections;

public sealed class ConnectionDirectory(TimeProvider clock)
{
    private readonly ConcurrentDictionary<Guid, LiveSession> sessions = new();
    private readonly ConcurrentDictionary<string, SocketPeer> listeners = new(StringComparer.Ordinal);
    private readonly AsyncLocal<MutationRecovery?> mutation = new();

    internal MutationRecovery BeginMutation()
    {
        var recovery = new MutationRecovery(this);
        mutation.Value = recovery;
        return recovery;
    }

    internal sealed class MutationRecovery(ConnectionDirectory directory) : IDisposable
    {
        private readonly HashSet<LiveSession> affected = [];
        internal void Track(LiveSession live) => affected.Add(live);
        internal void Recover()
        {
            foreach (var live in affected)
                lock (live.Sync) live.Host?.Abort();
        }
        public void Dispose() => directory.mutation.Value = null;
    }

    public bool HostOnline(Guid id) => sessions.TryGetValue(id, out var live) && live.Host?.IsOpen == true;

    internal SocketPeer[] OnlinePeers()
    {
        var peers = new HashSet<SocketPeer>(listeners.Values);
        foreach (var live in sessions.Values)
            lock (live.Sync)
            {
                if (live.Host is { } host) peers.Add(host);
                foreach (var participant in live.Participants.Values) peers.Add(participant.Peer);
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

    public void Invalidate(Session state, bool notifyHost = true, Func<Guid, string, bool>? keep = null)
    {
        if (!sessions.TryGetValue(state.Id, out var live)) return;
        lock (live.Sync)
        {
            mutation.Value?.Track(live);
            live.Ready = false; live.Revision = state.AuthorizationRevision; live.Generation = state.KeyGeneration;
            live.Order.Clear(); live.Pending.Clear(); live.Checkpoints.Clear();
            foreach (var participant in live.Participants.Values.ToArray())
            {
                var peer = participant.Peer;
                if (peer.IsOpen && keep?.Invoke(peer.UserId, peer.DeviceId) == true)
                {
                    participant.Ready = false; participant.Resync = false; participant.Rekey = true;
                    participant.BootstrapGeneration++; participant.LastSequence = 0;
                    continue;
                }
                peer.Abort(); live.Participants.Remove(peer.ConnectionId);
                live.Host?.Send(new { type = "participantDisconnected", connectionId = peer.ConnectionId, senderUserId = peer.UserId, senderDeviceId = peer.DeviceId });
            }
            if (notifyHost) live.Host?.Send(new { type = "accessChanged", authorizationRevision = live.Revision, keyGeneration = live.Generation });
        }
    }

    public void MarkReady(Session state)
    {
        if (!sessions.TryGetValue(state.Id, out var live)) return;
        lock (live.Sync)
        {
            if (live.IncarnationId != state.IncarnationId) return;
            live.Revision = state.AuthorizationRevision; live.Generation = state.KeyGeneration; live.Ready = state.Ready;
            if (!live.Ready) return;
            foreach (var participant in live.Participants.Values.Where(x => x.Rekey))
            {
                participant.Rekey = false; participant.Resync = true;
                participant.Peer.Send(new { type = "rekey", authorizationRevision = live.Revision, keyGeneration = live.Generation });
            }
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
                if (live.Host?.UserId == userId && live.Host.DeviceId == deviceId) live.Host.Abort();
                foreach (var participant in live.Participants.Values.Where(x => x.Peer.UserId == userId && x.Peer.DeviceId == deviceId)) participant.Peer.Abort();
            }
    }
    internal async Task EndSessionAsync(Guid id, SocketPeer? host = null, ulong? finalSequence = null)
    {
        if (!sessions.TryGetValue(id, out var live)) return;
        SocketPeer[] viewers;
        ulong last;
        lock (live.Sync)
        {
            if (host is not null && !ReferenceEquals(live.Host, host)) throw ApiException.Conflict("Host changed.");
            last = live.Order.NextSequence ?? 0;
            if (finalSequence is { } expected && expected != last) throw ApiException.Conflict("Terminal end is out of order.");
            live.Ending = true;
            viewers = live.Participants.Values.Where(x => x.Ready).Select(x => x.Peer).ToArray();
        }
        await Task.WhenAll(viewers.Select(peer => peer.DrainAndCloseAsync(new { type = "ended", finalSequence = last })));
    }

    public void RemoveSession(Guid id)
    {
        if (!sessions.TryRemove(id, out var live)) return;
        lock (live.Sync)
        {
            live.Ready = false; live.Host?.Abort();
            foreach (var participant in live.Participants.Values) participant.Peer.Abort();
            live.Participants.Clear(); live.Pending.Clear(); live.Checkpoints.Clear(); live.Order.Clear();
        }
    }
    public void StopAll()
    {
        foreach (var id in sessions.Keys) RemoveSession(id);
        foreach (var peer in listeners.Values) peer.Abort();
        listeners.Clear();
    }

    internal LiveSession RegisterHost(Session state, SocketPeer host)
    {
        if (sessions.Count >= 4096 && !sessions.ContainsKey(state.Id)) throw new ApiException(503, "Terminal connection capacity reached.");
        LiveSession live;
        while (true)
        {
            live = sessions.GetOrAdd(state.Id, _ => new LiveSession(state.Id, state.IncarnationId));
            lock (live.Sync)
            {
                if (!sessions.TryGetValue(state.Id, out var current) || !ReferenceEquals(current, live)) continue;
                if (live.IncarnationId != state.IncarnationId) throw ApiException.Conflict("Session publication changed.");
                live.Ready = false;
                live.Host?.Abort();
                foreach (var participant in live.Participants.Values) participant.Peer.Abort();
                live.Participants.Clear(); live.Pending.Clear(); live.Checkpoints.Clear(); live.Order.Clear();
                live.Host = host; live.Revision = state.AuthorizationRevision; live.Generation = state.KeyGeneration;
                break;
            }
        }
        return live;
    }
    internal LiveSession RegisterParticipant(Session state, SocketPeer peer)
    {
        if (!sessions.TryGetValue(state.Id, out var live)) throw new ApiException(409, "The host is offline.");
        lock (live.Sync)
        {
            if (live.Ending || live.IncarnationId != state.IncarnationId || !state.Ready || !live.Ready || live.Host?.IsOpen != true
                || live.Revision != state.AuthorizationRevision || live.Generation != state.KeyGeneration)
                throw ApiException.Conflict("The host is refreshing session access.");
            if (live.Participants.Count >= 64) throw new ApiException(503, "Too many connected participants.");
            var participant = new Participant(peer);
            live.Participants.Add(peer.ConnectionId, participant);
        }
        return live;
    }
    internal void BeginBootstrap(LiveSession live, SocketPeer peer, string? challenge, bool resync = false)
    {
        var verifiedChallenge = CheckpointChallenge(challenge);
        lock (live.Sync)
        {
            if (!live.Ready || !live.Participants.TryGetValue(peer.ConnectionId, out var participant) || participant.Resync != resync) return;
            if (resync && verifiedChallenge == participant.CheckpointChallenge) throw ApiException.Invalid("Use a new checkpoint challenge.");
            participant.Resync = false;
            participant.CheckpointChallenge = verifiedChallenge;
            participant.Ready = false;
            participant.BootstrapGeneration++;
            participant.BootstrapAttempts = 0;
            foreach (var id in live.Checkpoints.Where(x => x.Value.ConnectionId == peer.ConnectionId).Select(x => x.Key).ToArray()) live.Checkpoints.Remove(id);
            RequestCheckpoint(live, participant);
        }
    }
    internal void RequestCapture(LiveSession live, SocketPeer peer, string? challenge)
    {
        var verifiedChallenge = CheckpointChallenge(challenge);
        lock (live.Sync)
        {
            if (!live.Ready || !live.Participants.TryGetValue(peer.ConnectionId, out var participant) || !participant.Ready
                || live.Checkpoints.Values.Any(x => x.ConnectionId == peer.ConnectionId)) return;
            if (verifiedChallenge == participant.CheckpointChallenge) throw ApiException.Invalid("Use a new checkpoint challenge.");
            participant.CheckpointChallenge = verifiedChallenge;
            participant.BootstrapAttempts = 0;
            RequestCheckpoint(live, participant, captureOnly: true);
        }
    }
    private void RequestCheckpoint(LiveSession live, Participant participant, bool captureOnly = false)
    {
        if (++participant.BootstrapAttempts > 3 || live.Host?.IsOpen != true) { participant.Peer.Abort(); return; }
        var id = Guid.CreateVersion7();
        live.Checkpoints[id] = new PendingCheckpoint(participant.Peer.ConnectionId, clock.GetUtcNow().AddSeconds(5), captureOnly, participant.CheckpointChallenge);
        live.Host.Send(new
        {
            type = "checkpointRequested",
            requestId = id,
            challenge = participant.CheckpointChallenge,
            connectionId = participant.Peer.ConnectionId,
            recipientUserId = participant.Peer.UserId,
            recipientDeviceId = participant.Peer.DeviceId
        });
    }
    private static string CheckpointChallenge(string? challenge)
    {
        var bytes = Limits.Base64(challenge, "Checkpoint challenge", 32);
        if (bytes.Length != 32 || Convert.ToBase64String(bytes) != challenge) throw ApiException.Invalid("Invalid checkpoint challenge.");
        return challenge;
    }
    internal void RemovePeer(LiveSession live, SocketPeer peer, bool host)
    {
        lock (live.Sync)
        {
            if (host)
            {
                if (!ReferenceEquals(live.Host, peer)) return;
                live.Host = null; live.Ready = false; live.Order.Clear(); live.Pending.Clear(); live.Checkpoints.Clear();
                foreach (var participant in live.Participants.Values) participant.Peer.Abort();
                live.Participants.Clear();
                sessions.TryRemove(new KeyValuePair<Guid, LiveSession>(live.Id, live));
            }
            else if (live.Participants.Remove(peer.ConnectionId))
            {
                foreach (var id in live.Pending.Where(x => x.Value.Peer == peer).Select(x => x.Key).ToArray()) live.Pending.Remove(id);
                foreach (var id in live.Checkpoints.Where(x => x.Value.ConnectionId == peer.ConnectionId).Select(x => x.Key).ToArray()) live.Checkpoints.Remove(id);
                live.Host?.Send(new { type = "participantDisconnected", connectionId = peer.ConnectionId, senderUserId = peer.UserId, senderDeviceId = peer.DeviceId });
            }
        }
    }
    internal void Resync(LiveSession live, SocketPeer host)
    {
        lock (live.Sync)
        {
            if (!ReferenceEquals(live.Host, host)) throw ApiException.Conflict("Host is not currently admitted.");
            if (!live.Ready || live.Ending) return;
            live.Order.Restart();
            foreach (var participant in live.Participants.Values) Resync(live, participant);
        }
    }
    private static void Resync(LiveSession live, Participant participant)
    {
        foreach (var id in live.Checkpoints.Where(x => x.Value.ConnectionId == participant.Peer.ConnectionId).Select(x => x.Key).ToArray()) live.Checkpoints.Remove(id);
        participant.Ready = false;
        participant.BootstrapGeneration++;
        if (participant.Resync) return;
        participant.Resync = true;
        participant.Peer.Send(new { type = "resync" });
    }

    internal void Output(LiveSession live, SocketPeer host, byte[] frame)
    {
        lock (live.Sync)
        {
            if (!ReferenceEquals(live.Host, host)) throw ApiException.Conflict("Host output is not currently admitted.");
            if (!live.Ready || live.Ending || OutputOrder.Header(frame, live.Generation) is not { } header) return;
            live.Order.Accept(header);
            var generation = live.Generation;
            foreach (var participant in live.Participants.Values.Where(x => x.Ready))
            {
                if (header.Type == 4 && header.Next <= participant.NextSequence) continue;
                if (header.Type == 4 && header.First != participant.NextSequence)
                { participant.Peer.Abort(); continue; }
                if (!participant.Peer.HasOutputRoom(frame.Length)) { Resync(live, participant); continue; }
                if (header.Type != 5) participant.NextSequence = header.Next;
                var bootstrapGeneration = participant.BootstrapGeneration;
                Func<bool> current = () => IsParticipantCurrent(live, participant, generation, bootstrapGeneration);
                if (header.Type == 4) participant.Peer.SendOutput(frame, header.Next, current); else participant.Peer.SendBinary(frame, current);
            }
        }
    }
    private static bool IsParticipantCurrent(LiveSession live, Participant participant, int generation, long bootstrapGeneration)
    {
        lock (live.Sync) return live.Ready && live.Generation == generation && participant.Ready && participant.BootstrapGeneration == bootstrapGeneration
            && live.Participants.TryGetValue(participant.Peer.ConnectionId, out var current) && ReferenceEquals(current, participant);
    }
    internal void Checkpoint(LiveSession live, SocketPeer host, Guid requestId, byte[] frame, string? signature)
    {
        if (Limits.Base64(signature, "Checkpoint signature", 3309).Length != 3309) throw ApiException.Invalid("Invalid checkpoint signature.");
        lock (live.Sync)
        {
            if (!ReferenceEquals(live.Host, host)) throw ApiException.Conflict("Host checkpoint is not currently admitted.");
            if (!live.Ready || !live.Checkpoints.Remove(requestId, out var requested)
                || !live.Participants.TryGetValue(requested.ConnectionId, out var participant)) return;
            if (requested.Deadline <= clock.GetUtcNow())
            {
                if (!requested.CaptureOnly) RequestCheckpoint(live, participant);
                return;
            }
            var generation = live.Generation;
            var bootstrapGeneration = participant.BootstrapGeneration;
            if (OutputOrder.Header(frame, generation) is not { Type: 3 } header) throw ApiException.Invalid("Expected a terminal checkpoint.");
            var proof = new
            {
                type = "checkpointProof",
                challenge = requested.Challenge,
                signature,
                sessionId = live.Id,
                incarnationId = live.IncarnationId,
                authorizationRevision = live.Revision,
                keyGeneration = generation,
                connectionId = participant.Peer.ConnectionId,
                recipientUserId = participant.Peer.UserId,
                recipientDeviceId = participant.Peer.DeviceId,
                nextSequence = header.Next,
                frameSha256 = Convert.ToBase64String(System.Security.Cryptography.SHA256.HashData(frame))
            };
            if (requested.CaptureOnly)
            {
                participant.Peer.SendCheckpoint(proof, frame, () => IsParticipantCurrent(live, participant, generation, bootstrapGeneration));
                return;
            }
            if (!live.Order.Follows(header)) { RequestCheckpoint(live, participant); return; }
            participant.Ready = true;
            participant.Admitted = true;
            live.Host?.Send(new { type = "participantConnected", connectionId = participant.Peer.ConnectionId,
                senderUserId = participant.Peer.UserId, senderDeviceId = participant.Peer.DeviceId });
            participant.NextSequence = header.Next;
            participant.Peer.SendCheckpoint(proof, frame, () => IsParticipantCurrent(live, participant, generation, bootstrapGeneration));
        }
    }
    internal void Control(LiveSession live, SocketPeer peer, JsonElement body)
    {
        lock (live.Sync)
        {
            if (!live.Participants.TryGetValue(peer.ConnectionId, out var participant) || !participant.Admitted)
                throw ApiException.Forbidden("Session control is not currently admitted.");
            var sequence = body.GetProperty("sequence").GetUInt64();
            var generation = body.GetProperty("keyGeneration").GetInt32();
            var requestId = body.GetProperty("requestId").GetGuid();
            if (live.Ending || !live.Ready || live.Host?.IsOpen != true || generation < live.Generation) return;
            if (sequence != participant.LastSequence + 1 || sequence == 0 || generation != live.Generation || requestId == Guid.Empty)
                throw ApiException.Forbidden("Stale or replayed control.");
            if (live.Pending.Count >= 256 || live.Pending.ContainsKey(requestId)) throw new ApiException(503, "Too many pending terminal controls.");
            var ciphertext = body.GetProperty("ciphertext").GetString() ?? "";
            var nonce = body.GetProperty("nonce").GetString() ?? "";
            if (Limits.Base64(nonce, "Control nonce", 12).Length != 12
                || Limits.Base64(ciphertext, "Control ciphertext", 2 * 1024 * 1024).Length < 16)
                throw ApiException.Invalid("Invalid encrypted terminal control.");
            participant.LastSequence = sequence;
            var pending = new PendingControl(peer, sequence, requestId, live.Revision, generation, clock.GetUtcNow().AddSeconds(10));
            live.Pending.Add(requestId, pending);
            live.Host.Send(new
            {
                type = "control",
                sessionId = live.Id,
                incarnationId = live.IncarnationId,
                connectionId = peer.ConnectionId,
                senderUserId = peer.UserId,
                senderDeviceId = peer.DeviceId,
                authorizationRevision = live.Revision,
                sequence,
                requestId,
                ciphertext,
                nonce,
                keyGeneration = generation
            },
                () => IsControlCurrent(live, pending));
        }
    }
    private bool IsControlCurrent(LiveSession live, PendingControl pending)
    {
        lock (live.Sync) return live.Ready && live.Host?.IsOpen == true && pending.Peer.IsOpen && pending.Deadline > clock.GetUtcNow()
            && live.Revision == pending.Revision && live.Generation == pending.Generation
            && live.Participants.TryGetValue(pending.Peer.ConnectionId, out var participant) && ReferenceEquals(participant.Peer, pending.Peer);
    }
    internal void Result(LiveSession live, SocketPeer host, JsonElement body)
    {
        lock (live.Sync)
        {
            if (!ReferenceEquals(live.Host, host)) throw ApiException.Forbidden();
            var requestId = body.GetProperty("requestId").GetGuid();
            if (!live.Ready || !live.Pending.TryGetValue(requestId, out var pending)) return;
            if (body.GetProperty("connectionId").GetString() != pending.Peer.ConnectionId || body.GetProperty("sequence").GetUInt64() != pending.Sequence
                || !IsControlCurrent(live, pending)) throw ApiException.Forbidden("Control result does not match the current request.");
            var nonce = body.GetProperty("nonce").GetString() ?? "";
            var ciphertext = body.GetProperty("ciphertext").GetString() ?? "";
            if (Limits.Base64(nonce, "Result nonce", 12).Length != 12 || Limits.Base64(ciphertext, "Result ciphertext", 2048).Length < 16)
                throw ApiException.Invalid("Invalid encrypted control result.");
            live.Pending.Remove(requestId);
            pending.Peer.SendUrgent(new { type = "controlResult", requestId, nonce, ciphertext }, () => IsControlCurrent(live, pending));
        }
    }
    public void Sweep()
    {
        foreach (var live in sessions.Values)
        {
            lock (live.Sync)
            {
                var now = clock.GetUtcNow();
                foreach (var id in live.Pending.Where(x => x.Value.Deadline <= now).Select(x => x.Key).ToArray())
                { live.Pending[id].Peer.Abort(); live.Pending.Remove(id); }
                foreach (var id in live.Checkpoints.Where(x => x.Value.Deadline <= now).Select(x => x.Key).ToArray())
                {
                    var requested = live.Checkpoints[id]; live.Checkpoints.Remove(id);
                    if (!requested.CaptureOnly && live.Participants.TryGetValue(requested.ConnectionId, out var participant)) RequestCheckpoint(live, participant);
                }
            }
        }
    }

    internal sealed class LiveSession(Guid id, Guid incarnationId)
    {
        public object Sync { get; } = new();
        public Guid Id { get; } = id;
        public Guid IncarnationId { get; } = incarnationId;
        public SocketPeer? Host;
        public long Revision;
        public int Generation;
        public bool Ready;
        public bool Ending;
        public OutputOrder Order { get; } = new();
        public Dictionary<string, Participant> Participants { get; } = new(StringComparer.Ordinal);
        public Dictionary<Guid, PendingControl> Pending { get; } = new();
        public Dictionary<Guid, PendingCheckpoint> Checkpoints { get; } = new();
    }
    internal sealed class Participant(SocketPeer peer)
    {
        public SocketPeer Peer { get; } = peer;
        public bool Ready;
        public bool Admitted;
        public bool Resync;
        public bool Rekey;
        public string CheckpointChallenge = "";
        public ulong LastSequence;
        public ulong NextSequence;
        public int BootstrapAttempts;
        public long BootstrapGeneration;
    }
    internal sealed record PendingControl(SocketPeer Peer, ulong Sequence, Guid RequestId, long Revision, int Generation, DateTimeOffset Deadline);
    internal sealed record PendingCheckpoint(string ConnectionId, DateTimeOffset Deadline, bool CaptureOnly, string Challenge);
}
