namespace Kodosi.Application;

public sealed class PendingHostFenceQueue
{
    private readonly Lock _lock = new();
    private readonly List<PendingHostFence> _pendingHostFences = [];
    private readonly int? _capacity;

    public PendingHostFenceQueue(int? capacity = null)
    {
        if (capacity is < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(capacity), capacity, "Capacity must be positive.");
        }

        _capacity = capacity;
    }

    public bool HasPending
    {
        get
        {
            lock (_lock)
            {
                return _pendingHostFences.Count > 0;
            }
        }
    }

    public bool TryQueue(PendingHostFence fence)
    {
        ArgumentNullException.ThrowIfNull(fence);

        lock (_lock)
        {
            if (fence.CoalesceKey is { } coalesceKey)
            {
                for (var index = _pendingHostFences.Count - 1; index >= 0; index--)
                {
                    if (string.Equals(_pendingHostFences[index].CoalesceKey, coalesceKey, StringComparison.Ordinal))
                    {
                        _pendingHostFences.RemoveAt(index);
                        break;
                    }
                }
            }

            if (_capacity is { } capacity && _pendingHostFences.Count >= capacity)
            {
                return false;
            }

            _pendingHostFences.Add(fence);
            return true;
        }
    }

    public PendingHostFence? Peek()
    {
        lock (_lock)
        {
            return _pendingHostFences.Count == 0 ? null : _pendingHostFences[0];
        }
    }

    public bool Remove(PendingHostFence fence)
    {
        lock (_lock)
        {
            if (_pendingHostFences.Count == 0 || !Equals(_pendingHostFences[0], fence))
            {
                return false;
            }

            _pendingHostFences.RemoveAt(0);
            return true;
        }
    }

    public bool Remove(string fenceId)
    {
        lock (_lock)
        {
            var index = _pendingHostFences.FindIndex(
                fence => string.Equals(fence.FenceId, fenceId, StringComparison.Ordinal));
            if (index < 0)
            {
                return false;
            }
            _pendingHostFences.RemoveAt(index);
            return true;
        }
    }

    public bool Contains(string fenceId)
    {
        lock (_lock)
        {
            return _pendingHostFences.Any(
                fence => string.Equals(fence.FenceId, fenceId, StringComparison.Ordinal));
        }
    }

    public IReadOnlyList<PendingHostFence> Snapshot()
    {
        lock (_lock)
        {
            return [.. _pendingHostFences];
        }
    }
}
