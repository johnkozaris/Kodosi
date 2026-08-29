using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class RealtimePersistenceRepairTracker(
    TimeProvider timeProvider,
    Action? requestRecovery = null,
    ILogger<RealtimePersistenceRepairTracker>? logger = null,
    int capacity = RealtimePersistenceRepairTracker.DefaultCapacity,
    TimeSpan? maximumAge = null)
{
    internal const int DefaultCapacity = 4096;
    internal static readonly TimeSpan DefaultMaximumAge = TimeSpan.FromMinutes(10);

    private readonly Lock _lock = new();
    private readonly TimeProvider _timeProvider = timeProvider;
    private readonly Action _requestRecovery = requestRecovery ?? (() => { });
    private readonly ILogger<RealtimePersistenceRepairTracker>? _logger = logger;
    private readonly int _capacity = ValidateCapacity(capacity);
    private readonly TimeSpan _maximumAge = ValidateMaximumAge(
        maximumAge ?? DefaultMaximumAge);
    private readonly Dictionary<HostReleaseRepair, DateTimeOffset> _hostReleases = [];
    private readonly Dictionary<ParticipantCountRepair, DateTimeOffset> _participantCounts = [];
    private int _recoveryRequested;

    public RealtimePersistenceRepairSnapshot Snapshot()
    {
        var requiresRecovery = false;
        RealtimePersistenceRepairSnapshot snapshot;
        lock (_lock)
        {
            var now = _timeProvider.GetUtcNow();
            var hostReleases = new List<HostReleaseRepair>(_hostReleases.Count);
            foreach (var (repair, firstSeen) in _hostReleases)
            {
                hostReleases.Add(repair);
                requiresRecovery |= now - firstSeen >= _maximumAge;
            }
            var participantCounts = new List<ParticipantCountRepair>(_participantCounts.Count);
            foreach (var (repair, firstSeen) in _participantCounts)
            {
                participantCounts.Add(repair);
                requiresRecovery |= now - firstSeen >= _maximumAge;
            }
            snapshot = new RealtimePersistenceRepairSnapshot(
                hostReleases,
                participantCounts);
        }
        if (requiresRecovery)
        {
            RequestRecovery("A realtime persistence repair exceeded its maximum age.");
        }
        return snapshot;
    }

    public bool MarkHostRelease(SessionId sessionId, string connectionId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(connectionId);
        return TryMark(
            _hostReleases,
            new HostReleaseRepair(sessionId, connectionId));
    }

    public void CompleteHostRelease(HostReleaseRepair repair)
    {
        lock (_lock)
        {
            _hostReleases.Remove(repair);
        }
    }

    public bool MarkParticipantCount(
        SessionId sessionId,
        DateTimeOffset sessionStartedAt,
        Guid runtimeIncarnationId) =>
        TryMark(
            _participantCounts,
            new ParticipantCountRepair(
                sessionId,
                sessionStartedAt,
                runtimeIncarnationId));

    public void CompleteParticipantCount(ParticipantCountRepair repair)
    {
        lock (_lock)
        {
            _participantCounts.Remove(repair);
        }
    }

    private bool TryMark<TKey>(
        Dictionary<TKey, DateTimeOffset> repairs,
        TKey repair)
        where TKey : notnull
    {
        var overflow = false;
        lock (_lock)
        {
            if (repairs.ContainsKey(repair))
            {
                return true;
            }
            if (_hostReleases.Count + _participantCounts.Count >= _capacity)
            {
                overflow = true;
            }
            else
            {
                repairs.Add(repair, _timeProvider.GetUtcNow());
            }
        }
        if (overflow)
        {
            RequestRecovery("Realtime persistence repair capacity was exhausted.");
            return false;
        }
        return true;
    }

    private void RequestRecovery(string message)
    {
        if (Interlocked.Exchange(ref _recoveryRequested, 1) != 0)
        {
            return;
        }
        _logger?.LogCritical(
            "{Message} Requesting process recovery so startup can reset durable realtime counters.",
            message);
        _requestRecovery();
    }

    private static int ValidateCapacity(int capacity)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(capacity);
        return capacity;
    }

    private static TimeSpan ValidateMaximumAge(TimeSpan maximumAge)
    {
        if (maximumAge <= TimeSpan.Zero)
        {
            throw new ArgumentOutOfRangeException(nameof(maximumAge));
        }
        return maximumAge;
    }
}

internal sealed record RealtimePersistenceRepairSnapshot(
    IReadOnlyList<HostReleaseRepair> HostReleases,
    IReadOnlyList<ParticipantCountRepair> ParticipantCounts);

internal readonly record struct HostReleaseRepair(
    SessionId SessionId,
    string ConnectionId);

internal readonly record struct ParticipantCountRepair(
    SessionId SessionId,
    DateTimeOffset SessionStartedAt,
    Guid RuntimeIncarnationId);
