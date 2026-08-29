using System.Collections.Concurrent;

namespace Kodosi.Host.Middleware;

public sealed class AuthenticatedUserProvisioningLock
{
    private readonly ConcurrentDictionary<string, Entry> _entries =
        new(StringComparer.Ordinal);

    public async ValueTask<IAsyncDisposable> AcquireAsync(
    IEnumerable<string> identityKeys,
    CancellationToken ct)
    {
        ArgumentNullException.ThrowIfNull(identityKeys);

        var orderedKeys = identityKeys
            .Distinct(StringComparer.Ordinal)
            .Order(StringComparer.Ordinal)
            .ToArray();
        if (orderedKeys.Length == 0)
        {
            throw new ArgumentException(
                "At least one identity key is required.",
                nameof(identityKeys));
        }

        var leases = new List<IAsyncDisposable>(orderedKeys.Length);
        try
        {
            foreach (var identityKey in orderedKeys)
            {
                leases.Add(await AcquireAsync(identityKey, ct));
            }
        }
        catch
        {
            await ReleaseAllAsync(leases);
            throw;
        }

        return new CompositeLease(leases);
    }

    private static async ValueTask ReleaseAllAsync(List<IAsyncDisposable> leases)
    {

        for (var i = leases.Count - 1; i >= 0; i--)
        {
            await leases[i].DisposeAsync();
        }
    }

    private async ValueTask<IAsyncDisposable> AcquireAsync(
        string identityKey,
        CancellationToken ct)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(identityKey);

        while (true)
        {
            var entry = _entries.GetOrAdd(identityKey, static _ => new Entry());
            lock (entry.StateLock)
            {
                if (entry.Retired)
                {
                    _entries.TryRemove(
                        new KeyValuePair<string, Entry>(identityKey, entry));
                    continue;
                }

                entry.ReferenceCount++;
            }

            try
            {
                await entry.Semaphore.WaitAsync(ct);
                return new Lease(this, identityKey, entry);
            }
            catch
            {
                ReleaseReference(identityKey, entry, releaseSemaphore: false);
                throw;
            }
        }
    }

    private void Release(string identityKey, Entry entry) =>
        ReleaseReference(identityKey, entry, releaseSemaphore: true);

    private void ReleaseReference(
        string identityKey,
        Entry entry,
        bool releaseSemaphore)
    {
        if (releaseSemaphore)
        {
            entry.Semaphore.Release();
        }

        var remove = false;
        lock (entry.StateLock)
        {
            entry.ReferenceCount--;
            if (entry.ReferenceCount == 0)
            {
                entry.Retired = true;
                remove = true;
            }
        }

        if (remove)
        {
            _entries.TryRemove(new KeyValuePair<string, Entry>(identityKey, entry));
            entry.Semaphore.Dispose();
        }
    }

    private sealed class Entry
    {
        public Lock StateLock { get; } = new();
        public SemaphoreSlim Semaphore { get; } = new(1, 1);
        public int ReferenceCount;
        public bool Retired;
    }

    private sealed class CompositeLease(List<IAsyncDisposable> leases) : IAsyncDisposable
    {
        private List<IAsyncDisposable>? _leases = leases;

        public async ValueTask DisposeAsync()
        {
            if (Interlocked.Exchange(ref _leases, null) is { } held)
            {
                await ReleaseAllAsync(held);
            }
        }
    }

    private sealed class Lease(
        AuthenticatedUserProvisioningLock owner,
        string identityKey,
        Entry entry) : IAsyncDisposable
    {
        private AuthenticatedUserProvisioningLock? _owner = owner;

        public ValueTask DisposeAsync()
        {
            Interlocked.Exchange(ref _owner, null)?.Release(identityKey, entry);
            return ValueTask.CompletedTask;
        }
    }
}
