namespace Kodosi.HostTests;

internal sealed class TestTimeProvider(DateTimeOffset now) : TimeProvider
{
    private DateTimeOffset _now = now;
    private long _timestamp;

    public override DateTimeOffset GetUtcNow() => _now;
    public override long TimestampFrequency => TimeSpan.TicksPerSecond;
    public override long GetTimestamp() => _timestamp;

    public void Advance(TimeSpan delta)
    {
        _now = _now.Add(delta);
        _timestamp += delta.Ticks;
    }
}
