using System.Collections.Concurrent;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Realtime;

public sealed class LiveSessionStateDirectory(TimeProvider? timeProvider = null) : ILiveSessionStateDirectory
{
    private readonly ConcurrentDictionary<SessionId, LiveSessionRuntime> _runtimes = new();
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    public LiveSessionPorts? TryGet(SessionId sessionId)
    {
        return _runtimes.TryGetValue(sessionId, out var runtime) ? runtime.Ports : null;
    }

    public bool TryClaimHost(
        SessionId sessionId,
        string connectionId,
        CancellationTokenSource hostLifetime,
        out LiveSessionPorts? ports,
        out LiveSessionCreationOwnership? creationOwnership)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(connectionId);
        ArgumentNullException.ThrowIfNull(hostLifetime);

        while (true)
        {
            if (_runtimes.TryGetValue(sessionId, out var existing))
            {
                ports = existing.Ports;
                creationOwnership = null;
                return ports.Host.TryClaimHost(connectionId, hostLifetime);
            }

            var candidate = new LiveSessionRuntime(sessionId, _timeProvider);
            if (!candidate.Ports.Host.TryClaimHost(connectionId, hostLifetime))
            {
                ports = null;
                creationOwnership = null;
                return false;
            }

            if (_runtimes.TryAdd(sessionId, candidate))
            {
                ports = candidate.Ports;
                creationOwnership = candidate.CreationOwnership;
                return true;
            }

            candidate.Ports.Host.ReleaseHost(connectionId);
        }
    }

    public bool RemoveIfSame(SessionId sessionId, LiveSessionPorts expected)
    {
        ArgumentNullException.ThrowIfNull(expected);
        if (!_runtimes.TryGetValue(sessionId, out var runtime)
            || !ReferenceEquals(runtime.Ports, expected))
        {
            return false;
        }

        return ((ICollection<KeyValuePair<SessionId, LiveSessionRuntime>>)_runtimes)
            .Remove(new KeyValuePair<SessionId, LiveSessionRuntime>(sessionId, runtime));
    }

    public bool RemoveIfOwned(
        SessionId sessionId,
        LiveSessionCreationOwnership creationOwnership)
    {
        ArgumentNullException.ThrowIfNull(creationOwnership);
        if (!_runtimes.TryGetValue(sessionId, out var runtime)
            || !ReferenceEquals(
                runtime.CreationOwnership,
                creationOwnership))
        {
            return false;
        }

        return ((ICollection<KeyValuePair<SessionId, LiveSessionRuntime>>)_runtimes)
            .Remove(new KeyValuePair<SessionId, LiveSessionRuntime>(sessionId, runtime));
    }

    public IReadOnlyList<SessionId> GetActiveSessions()
    {
        return _runtimes.Keys.ToList();
    }
}
