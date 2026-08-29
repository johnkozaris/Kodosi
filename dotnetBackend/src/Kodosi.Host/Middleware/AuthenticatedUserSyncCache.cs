using System.Collections.Concurrent;
using System.Text;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Middleware;

public sealed class AuthenticatedUserSyncCache
{
    private const int DefaultMaxEntries = 50_000;

    private readonly ConcurrentDictionary<string, CacheEntry> _entries = new(StringComparer.Ordinal);
    private readonly Lock _writeLock = new();
    private readonly int _maxEntries;

    public AuthenticatedUserSyncCache(int maxEntries = DefaultMaxEntries)
    {
        _maxEntries = maxEntries > 0
            ? maxEntries
            : throw new ArgumentOutOfRangeException(nameof(maxEntries));
    }

    internal int Count => _entries.Count;

    public bool TryGetFresh(
        AuthenticatedUserProfile profile,
        TimeSpan ttl,
        DateTimeOffset now,
        out CachedUserProfile cached)
    {
        cached = default!;
        var cacheKey = BuildCacheKey(profile);
        var fingerprint = BuildFingerprint(profile);
        if (!_entries.TryGetValue(cacheKey, out var existing))
        {
            return false;
        }

        if (!string.Equals(existing.Fingerprint, fingerprint, StringComparison.Ordinal))
        {
            RemoveIfSame(cacheKey, existing);
            return false;
        }

        if (now - existing.LastSyncedAt >= ttl)
        {
            RemoveIfSame(cacheKey, existing);
            return false;
        }

        cached = new CachedUserProfile(existing.UserId, existing.Handle);
        return true;
    }

    public void RecordSync(
        AuthenticatedUserProfile profile,
        UserId userId,
        string handle,
        DateTimeOffset now,
        TimeSpan ttl)
    {
        var key = BuildCacheKey(profile);
        var entry = new CacheEntry(
                BuildFingerprint(profile),
                userId.Value,
                handle,
                now);
        lock (_writeLock)
        {
            if (!_entries.ContainsKey(key))
            {
                SweepExpired(now, ttl);
                while (_entries.Count >= _maxEntries)
                {
                    var oldest = _entries.MinBy(pair => pair.Value.LastSyncedAt);
                    if (oldest.Key is null
                        || !_entries.TryRemove(oldest.Key, out _))
                    {
                        break;
                    }
                }
            }

            _entries[key] = entry;
        }
    }

    private static string BuildFingerprint(AuthenticatedUserProfile profile)
    {
        var builder = new StringBuilder();
        foreach (var identity in profile.Identities)
        {
            AppendFramed(builder, identity.IdentityKey);
        }

        AppendFramed(builder, profile.HandleSeed);
        AppendFramed(builder, profile.Email ?? string.Empty);
        AppendFramed(builder, profile.DisplayName ?? string.Empty);
        AppendFramed(builder, profile.AvatarUrl ?? string.Empty);

        return builder.ToString();
    }

    private static string BuildCacheKey(AuthenticatedUserProfile profile) =>
        profile.PrimaryIdentity.IdentityKey;

    private static void AppendFramed(StringBuilder builder, string value)
    {
        builder.Append(Encoding.UTF8.GetByteCount(value))
            .Append(':')
            .Append(value);
    }

    public readonly record struct CachedUserProfile(Guid UserId, string Handle);

    private sealed record CacheEntry(
        string Fingerprint,
        Guid UserId,
        string Handle,
        DateTimeOffset LastSyncedAt);

    private void SweepExpired(DateTimeOffset now, TimeSpan ttl)
    {
        foreach (var (key, entry) in _entries)
        {
            if (now - entry.LastSyncedAt >= ttl)
            {
                RemoveIfSame(key, entry);
            }
        }
    }

    private void RemoveIfSame(string key, CacheEntry entry) =>
        _entries.TryRemove(new KeyValuePair<string, CacheEntry>(key, entry));
}
