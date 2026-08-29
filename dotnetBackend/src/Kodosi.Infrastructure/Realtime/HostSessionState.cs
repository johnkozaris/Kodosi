using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.Realtime;

internal sealed class HostSessionState : ILiveSessionHostState
{
    private const int PendingHostFenceCapacity = 1024;

    private readonly Lock _lock = new();
    private readonly PendingHostFenceQueue _pendingHostFences = new(PendingHostFenceCapacity);
    private readonly TimeProvider _timeProvider;
    private string? _hostConnectionId;
    private CancellationTokenSource? _hostLifetimeCts;
    private SessionStatus _status = SessionStatus.Pending;
    private bool _hostConnected;
    private bool _hostReady;
    private DateTimeOffset? _sessionStartedAt;
    private Guid _sessionIncarnationId;
    private long _lastHostHeartbeatTimestamp;
    private long? _disconnectedSinceTimestamp;

    public HostSessionState(TimeProvider? timeProvider = null)
    {
        _timeProvider = timeProvider ?? TimeProvider.System;
        _lastHostHeartbeatTimestamp = Timestamp;
    }

    public SessionStatus Status
    {
        get
        {
            lock (_lock)
            {
                return _status;
            }
        }
    }

    public bool HostConnected
    {
        get
        {
            lock (_lock)
            {
                return _hostConnected;
            }
        }
    }

    public bool HostReady
    {
        get
        {
            lock (_lock)
            {
                return _hostReady;
            }
        }
    }

    public string? HostConnectionId
    {
        get
        {
            lock (_lock)
            {
                return _hostConnectionId;
            }
        }
    }

    public DateTimeOffset? SessionStartedAt
    {
        get
        {
            lock (_lock)
            {
                return _sessionStartedAt;
            }
        }
    }

    public Guid SessionIncarnationId
    {
        get
        {
            lock (_lock)
            {
                return _sessionIncarnationId;
            }
        }
    }

    private long Timestamp => _timeProvider.GetTimestamp();

    public void SetStatus(SessionStatus status)
    {
        lock (_lock)
        {
            _status = status;
        }
    }

    public bool TryClaimHost(string connectionId, CancellationTokenSource hostLifetimeCts)
    {
        lock (_lock)
        {
            if (_hostConnectionId is not null)
            {
                return false;
            }

            _hostConnectionId = connectionId;
            _hostLifetimeCts = hostLifetimeCts;
            _hostConnected = true;
            _hostReady = false;
            _lastHostHeartbeatTimestamp = Timestamp;
            _disconnectedSinceTimestamp = null;
            return true;
        }
    }

    public void ReleaseHost(string connectionId)
    {
        lock (_lock)
        {
            if (!string.Equals(_hostConnectionId, connectionId, StringComparison.Ordinal))
            {
                return;
            }

            _hostConnectionId = null;
            _hostLifetimeCts = null;
            _hostConnected = false;
            _hostReady = false;
            _disconnectedSinceTimestamp = Timestamp;
        }
    }

    public bool IsHostHeartbeatPast(string connectionId, TimeSpan timeout)
    {
        lock (_lock)
        {
            return string.Equals(
                    _hostConnectionId,
                    connectionId,
                    StringComparison.Ordinal)
                && _hostConnected
                && _timeProvider.GetElapsedTime(
                    _lastHostHeartbeatTimestamp,
                    Timestamp) > timeout;
        }
    }

    public bool TryTimeoutHost(string connectionId, TimeSpan timeout)
    {
        CancellationTokenSource? hostLifetimeCts = null;
        lock (_lock)
        {
            if (!string.Equals(
                    _hostConnectionId,
                    connectionId,
                    StringComparison.Ordinal)
                || !_hostConnected
                || _timeProvider.GetElapsedTime(_lastHostHeartbeatTimestamp, Timestamp) <= timeout)
            {
                return false;
            }

            _hostConnectionId = null;
            hostLifetimeCts = _hostLifetimeCts;
            _hostLifetimeCts = null;
            _hostConnected = false;
            _hostReady = false;
            _disconnectedSinceTimestamp = Timestamp;
        }

        hostLifetimeCts?.Cancel();
        return true;
    }

    public void SetSessionStartedAt(DateTimeOffset startedAt)
    {
        lock (_lock)
        {
            _sessionStartedAt = startedAt;
        }
    }

    public void SetSessionIncarnationId(Guid incarnationId)
    {
        lock (_lock)
        {
            _sessionIncarnationId = incarnationId;
        }
    }

    public bool IsDisconnectedPast(TimeSpan timeout)
    {
        lock (_lock)
        {
            return !_hostConnected
                && _disconnectedSinceTimestamp is { } disconnectedSince
                && _timeProvider.GetElapsedTime(disconnectedSince, Timestamp) > timeout;
        }
    }

    public void SetHostReady(bool ready)
    {
        lock (_lock)
        {
            _hostReady = ready;
        }
    }

    public void RecordHostHeartbeat()
    {
        lock (_lock)
        {
            _lastHostHeartbeatTimestamp = Timestamp;
        }
    }

    public bool HasPendingHostFences
    {
        get => _pendingHostFences.HasPending;
    }

    public bool TryQueuePendingHostFence(PendingHostFence fence)
    {
        return _pendingHostFences.TryQueue(fence);
    }

    public PendingHostFence? PeekPendingHostFence()
    {
        return _pendingHostFences.Peek();
    }

    public bool RemovePendingHostFence(PendingHostFence fence)
    {
        return _pendingHostFences.Remove(fence);
    }
}
