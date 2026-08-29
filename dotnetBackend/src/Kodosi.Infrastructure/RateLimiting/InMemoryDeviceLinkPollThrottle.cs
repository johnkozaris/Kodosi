using System.Collections.Concurrent;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Application;

namespace Kodosi.Infrastructure.RateLimiting;

public sealed class InMemoryDeviceLinkPollThrottle(
    int permitLimit,
    TimeSpan window,
    TimeProvider? timeProvider = null) : IDeviceLinkPollThrottle
{
    private readonly ConcurrentDictionary<string, WindowState> _windows =
        new(StringComparer.Ordinal);
    private readonly int _permitLimit = permitLimit > 0
        ? permitLimit
        : throw new ArgumentOutOfRangeException(nameof(permitLimit));
    private readonly TimeSpan _window = window > TimeSpan.Zero
        ? window
        : throw new ArgumentOutOfRangeException(nameof(window));
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    public TimeSpan? TryClaim(string deviceCode)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(deviceCode);
        var key = Hash(deviceCode);
        var now = _timeProvider.GetUtcNow();
        while (true)
        {
            if (!_windows.TryGetValue(key, out var existing))
            {
                if (_windows.TryAdd(key, new WindowState(now, 1)))
                {
                    return null;
                }
                continue;
            }

            var elapsed = now - existing.StartedAt;
            if (elapsed >= _window)
            {
                if (_windows.TryUpdate(key, new WindowState(now, 1), existing))
                {
                    return null;
                }
                continue;
            }
            if (existing.Count >= _permitLimit)
            {
                return _window - elapsed;
            }
            if (_windows.TryUpdate(key, existing with { Count = existing.Count + 1 }, existing))
            {
                return null;
            }
        }
    }

    public void Sweep()
    {
        var cutoff = _timeProvider.GetUtcNow() - _window;
        foreach (var entry in _windows)
        {
            if (entry.Value.StartedAt <= cutoff)
            {
                _windows.TryRemove(entry);
            }
        }
    }

    private static string Hash(string deviceCode) =>
        Convert.ToHexStringLower(
            SHA256.HashData(Encoding.UTF8.GetBytes(deviceCode)));

    private sealed record WindowState(DateTimeOffset StartedAt, int Count);
}
