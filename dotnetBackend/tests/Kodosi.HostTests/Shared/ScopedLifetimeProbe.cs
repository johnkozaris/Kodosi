namespace Kodosi.HostTests;

internal sealed class ScopedLifetimeProbe
{
    private int _created;
    private int _disposed;

    public int Created => Volatile.Read(ref _created);
    public int Disposed => Volatile.Read(ref _disposed);
    public int Active => Created - Disposed;

    public ScopedLifetimeLease CreateLease()
    {
        Interlocked.Increment(ref _created);
        return new ScopedLifetimeLease(this);
    }

    internal void RecordDisposed() =>
        Interlocked.Increment(ref _disposed);
}

internal sealed class ScopedLifetimeLease(
    ScopedLifetimeProbe probe) : IAsyncDisposable
{
    private readonly ScopedLifetimeProbe _probe = probe;
    private int _disposed;

    public ValueTask DisposeAsync()
    {
        if (Interlocked.Exchange(ref _disposed, 1) == 0)
        {
            _probe.RecordDisposed();
        }

        return ValueTask.CompletedTask;
    }
}
