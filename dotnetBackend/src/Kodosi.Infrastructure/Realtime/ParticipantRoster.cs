using Kodosi.Application;

namespace Kodosi.Infrastructure.Realtime;

internal sealed class ParticipantRoster(TimeProvider? timeProvider = null) : ILiveParticipantRoster, ILiveStreamDemand
{
    private readonly Lock _lock = new();
    private readonly Dictionary<string, long> _sharedParticipants = new(StringComparer.Ordinal);
    private readonly Dictionary<string, long> _ownerParticipants = new(StringComparer.Ordinal);
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    private long Timestamp => _timeProvider.GetTimestamp();

    public StreamDemandSnapshot GetStreamDemand()
    {
        lock (_lock)
        {
            return CreateLiveDemandSnapshotUnsafe();
        }
    }

    public bool TryAddSharedParticipant(
        string connectionId,
        int maxSharedParticipantCount,
        out StreamDemandTransition transition)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(maxSharedParticipantCount);

        lock (_lock)
        {
            var previous = CreateLiveDemandSnapshotUnsafe();
            if (!_sharedParticipants.ContainsKey(connectionId) && _sharedParticipants.Count >= maxSharedParticipantCount)
            {
                transition = new StreamDemandTransition(previous, previous);
                return false;
            }

            _sharedParticipants[connectionId] = Timestamp;
            var current = CreateLiveDemandSnapshotUnsafe();
            transition = new StreamDemandTransition(previous, current);
            return true;
        }
    }

    public StreamDemandTransition RemoveSharedParticipant(string connectionId)
    {
        lock (_lock)
        {
            var previous = CreateLiveDemandSnapshotUnsafe();
            _sharedParticipants.Remove(connectionId);
            var current = CreateLiveDemandSnapshotUnsafe();
            return new StreamDemandTransition(previous, current);
        }
    }

    public bool TryRemoveSharedParticipantIfStale(
        string connectionId,
        TimeSpan timeout,
        out StreamDemandTransition transition)
    {
        lock (_lock)
        {
            return TryRemoveIfStale(
                _sharedParticipants,
                connectionId,
                timeout,
                out transition);
        }
    }

    public bool TryAddOwnerParticipant(
        string connectionId,
        int maxOwnerParticipantCount,
        out StreamDemandTransition transition)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(
            maxOwnerParticipantCount);
        lock (_lock)
        {
            var previous = CreateLiveDemandSnapshotUnsafe();
            if (!_ownerParticipants.ContainsKey(connectionId)
                && _ownerParticipants.Count >= maxOwnerParticipantCount)
            {
                transition = new StreamDemandTransition(previous, previous);
                return false;
            }

            _ownerParticipants[connectionId] = Timestamp;
            var current = CreateLiveDemandSnapshotUnsafe();
            transition = new StreamDemandTransition(previous, current);
            return true;
        }
    }

    public StreamDemandTransition RemoveOwnerParticipant(string connectionId)
    {
        lock (_lock)
        {
            var previous = CreateLiveDemandSnapshotUnsafe();
            _ownerParticipants.Remove(connectionId);
            var current = CreateLiveDemandSnapshotUnsafe();
            return new StreamDemandTransition(previous, current);
        }
    }

    public bool TryRemoveOwnerParticipantIfStale(
        string connectionId,
        TimeSpan timeout,
        out StreamDemandTransition transition)
    {
        lock (_lock)
        {
            return TryRemoveIfStale(
                _ownerParticipants,
                connectionId,
                timeout,
                out transition);
        }
    }

    public void RecordParticipantActivity(string connectionId)
    {
        lock (_lock)
        {
            if (_sharedParticipants.ContainsKey(connectionId))
            {
                _sharedParticipants[connectionId] = Timestamp;
                return;
            }

            if (_ownerParticipants.ContainsKey(connectionId))
            {
                _ownerParticipants[connectionId] = Timestamp;
            }
        }
    }

    public IReadOnlyList<string> GetStaleSharedParticipantConnectionIds(TimeSpan timeout)
    {
        lock (_lock)
        {
            var now = Timestamp;
            return _sharedParticipants
                .Where(kvp => _timeProvider.GetElapsedTime(kvp.Value, now) > timeout)
                .Select(kvp => kvp.Key)
                .ToList();
        }
    }

    public IReadOnlyList<string> GetStaleOwnerParticipantConnectionIds(TimeSpan timeout)
    {
        lock (_lock)
        {
            var now = Timestamp;
            return _ownerParticipants
                .Where(kvp => _timeProvider.GetElapsedTime(kvp.Value, now) > timeout)
                .Select(kvp => kvp.Key)
                .ToList();
        }
    }

    private StreamDemandSnapshot CreateLiveDemandSnapshotUnsafe()
    {
        return new StreamDemandSnapshot(_sharedParticipants.Count, _ownerParticipants.Count);
    }

    private bool TryRemoveIfStale(
        Dictionary<string, long> participants,
        string connectionId,
        TimeSpan timeout,
        out StreamDemandTransition transition)
    {
        var previous = CreateLiveDemandSnapshotUnsafe();
        if (!participants.TryGetValue(connectionId, out var lastActivity)
            || _timeProvider.GetElapsedTime(lastActivity, Timestamp) <= timeout)
        {
            transition = new StreamDemandTransition(previous, previous);
            return false;
        }

        participants.Remove(connectionId);
        var current = CreateLiveDemandSnapshotUnsafe();
        transition = new StreamDemandTransition(previous, current);
        return true;
    }
}
