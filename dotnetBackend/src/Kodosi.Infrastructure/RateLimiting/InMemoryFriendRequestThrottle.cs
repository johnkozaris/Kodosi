using System.Collections.Concurrent;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Infrastructure.RateLimiting;

public sealed class InMemoryFriendRequestThrottle(
    TimeProvider? clock = null,
    int maxEntries = 100_000) : IFriendRequestThrottle
{
    private static readonly TimeSpan Window = TimeSpan.FromHours(24);
    private static readonly TimeSpan CapacityRetryAfter = TimeSpan.FromSeconds(1);

    private readonly ConcurrentDictionary<(Guid SenderId, Guid TargetId), DateTimeOffset> _lastSendAt = new();
    private readonly Lock _admissionLock = new();
    private readonly TimeProvider _clock = clock ?? TimeProvider.System;
    private readonly int _maxEntries = maxEntries > 0
        ? maxEntries
        : throw new ArgumentOutOfRangeException(nameof(maxEntries), maxEntries, "Capacity must be positive.");

    public TimeSpan? TryClaim(UserId senderId, UserId targetId)
    {
        lock (_admissionLock)
        {
            var now = _clock.GetUtcNow();
            var key = (senderId.Value, targetId.Value);

            if (_lastSendAt.TryGetValue(key, out var existing))
            {
                var elapsed = now - existing;
                if (elapsed < Window)
                {
                    return Window - elapsed;
                }

                _lastSendAt[key] = now;
                return null;
            }

            if (_lastSendAt.Count >= _maxEntries)
            {
                SweepExpired(now);
                if (_lastSendAt.Count >= _maxEntries)
                {
                    return CapacityRetryAfter;
                }
            }

            _lastSendAt[key] = now;
            return null;
        }
    }

    public void Release(UserId senderId, UserId targetId)
    {
        lock (_admissionLock)
        {
            var key = (senderId.Value, targetId.Value);
            _lastSendAt.TryRemove(key, out _);
        }
    }

    public void Sweep()
    {
        lock (_admissionLock)
        {
            SweepExpired(_clock.GetUtcNow());
        }
    }

    private void SweepExpired(DateTimeOffset now)
    {
        var cutoff = now - Window;
        foreach (var (key, last) in _lastSendAt)
        {
            if (last < cutoff)
            {
                _lastSendAt.TryRemove(new KeyValuePair<(Guid, Guid), DateTimeOffset>(key, last));
            }
        }
    }
}
