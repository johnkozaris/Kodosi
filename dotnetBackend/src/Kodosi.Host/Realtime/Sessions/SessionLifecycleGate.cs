using System.Collections.Concurrent;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class SessionLifecycleGate
{
    private readonly ConcurrentDictionary<SessionId, Entry> _entries = new();

    public async ValueTask<IAsyncDisposable> AcquireAsync(
        SessionId sessionId,
        CancellationToken ct)
    {
        while (true)
        {
            var entry = _entries.GetOrAdd(sessionId, static _ => new Entry());
            lock (entry.StateLock)
            {
                if (entry.Retired)
                {
                    _entries.TryRemove(
                        new KeyValuePair<SessionId, Entry>(sessionId, entry));
                    continue;
                }

                entry.ReferenceCount++;
            }

            try
            {
                await entry.Semaphore.WaitAsync(ct);
                return new Lease(this, sessionId, entry);
            }
            catch
            {
                ReleaseReference(sessionId, entry, releaseSemaphore: false);
                throw;
            }
        }
    }

    public async ValueTask<IAsyncDisposable> AcquireAsync(
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct)
    {
        var leases = new List<IAsyncDisposable>();
        try
        {
            foreach (var sessionId in sessionIds
                .Distinct()
                .OrderBy(static id => id.Value))
            {
                leases.Add(await AcquireAsync(sessionId, ct));
            }

            return new CompositeLease(leases);
        }
        catch
        {
            await DisposeReverseAsync(leases);
            throw;
        }
    }

    private static async ValueTask DisposeReverseAsync(
        IReadOnlyList<IAsyncDisposable> leases)
    {
        for (var index = leases.Count - 1; index >= 0; index--)
        {
            await leases[index].DisposeAsync();
        }
    }

    private void Release(SessionId sessionId, Entry entry) =>
        ReleaseReference(sessionId, entry, releaseSemaphore: true);

    private void ReleaseReference(
        SessionId sessionId,
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
            _entries.TryRemove(
                new KeyValuePair<SessionId, Entry>(sessionId, entry));
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

    private sealed class Lease(
        SessionLifecycleGate owner,
        SessionId sessionId,
        Entry entry) : IAsyncDisposable
    {
        private SessionLifecycleGate? _owner = owner;

        public ValueTask DisposeAsync()
        {
            Interlocked.Exchange(ref _owner, null)?.Release(sessionId, entry);
            return ValueTask.CompletedTask;
        }
    }

    private sealed class CompositeLease(
        IReadOnlyList<IAsyncDisposable> leases) : IAsyncDisposable
    {
        private IReadOnlyList<IAsyncDisposable>? _leases = leases;

        public async ValueTask DisposeAsync()
        {
            var leasesToDispose = Interlocked.Exchange(ref _leases, null);
            if (leasesToDispose is not null)
            {
                await DisposeReverseAsync(leasesToDispose);
            }
        }
    }
}
