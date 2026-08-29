using Kodosi.Domain;
using Kodosi.Host.Realtime;

namespace Kodosi.HostTests;

public sealed class RealtimePersistenceRepairTrackerTests
{
    [Fact]
    public void Capacity_Is_Global_And_Overflow_Requests_Recovery_Without_Eviction()
    {
        var recoveryRequests = 0;
        var tracker = new RealtimePersistenceRepairTracker(
            new TestTimeProvider(DateTimeOffset.UnixEpoch),
            () => recoveryRequests++,
            capacity: 2);
        var host = new HostReleaseRepair(SessionId.New(), "host");
        var participant = new ParticipantCountRepair(
            SessionId.New(),
            DateTimeOffset.UnixEpoch,
            Guid.NewGuid());

        Assert.True(tracker.MarkHostRelease(host.SessionId, host.ConnectionId));
        Assert.True(tracker.MarkParticipantCount(
            participant.SessionId,
            participant.SessionStartedAt,
            participant.RuntimeIncarnationId));
        Assert.False(tracker.MarkHostRelease(SessionId.New(), "overflow"));
        Assert.False(tracker.MarkParticipantCount(
            SessionId.New(),
            DateTimeOffset.UnixEpoch,
            Guid.NewGuid()));

        var snapshot = tracker.Snapshot();
        Assert.Equal([host], snapshot.HostReleases);
        Assert.Equal([participant], snapshot.ParticipantCounts);
        Assert.Equal(1, recoveryRequests);
    }

    [Fact]
    public void Duplicate_Does_Not_Renew_Age_And_Expiry_Remains_Retryable()
    {
        var recoveryRequests = 0;
        var clock = new TestTimeProvider(DateTimeOffset.UnixEpoch);
        var tracker = new RealtimePersistenceRepairTracker(
            clock,
            () => recoveryRequests++,
            maximumAge: TimeSpan.FromMinutes(10));
        var sessionId = SessionId.New();

        Assert.True(tracker.MarkHostRelease(sessionId, "host"));
        clock.Advance(TimeSpan.FromMinutes(9));
        Assert.True(tracker.MarkHostRelease(sessionId, "host"));
        clock.Advance(TimeSpan.FromMinutes(1));

        Assert.Equal(
            [new HostReleaseRepair(sessionId, "host")],
            tracker.Snapshot().HostReleases);
        Assert.Equal(1, recoveryRequests);
        Assert.Single(tracker.Snapshot().HostReleases);
        Assert.Equal(1, recoveryRequests);
    }

    [Fact]
    public void Completion_Frees_Capacity_And_Prevents_Expiry_Recovery()
    {
        var recoveryRequests = 0;
        var clock = new TestTimeProvider(DateTimeOffset.UnixEpoch);
        var tracker = new RealtimePersistenceRepairTracker(
            clock,
            () => recoveryRequests++,
            capacity: 1,
            maximumAge: TimeSpan.FromMinutes(10));
        var first = new HostReleaseRepair(SessionId.New(), "host");

        var replacement = new HostReleaseRepair(SessionId.New(), "replacement");
        Assert.True(tracker.MarkHostRelease(first.SessionId, first.ConnectionId));
        tracker.CompleteHostRelease(first);
        Assert.True(tracker.MarkHostRelease(
            replacement.SessionId,
            replacement.ConnectionId));
        clock.Advance(TimeSpan.FromMinutes(9));

        Assert.Equal([replacement], tracker.Snapshot().HostReleases);
        Assert.Equal(0, recoveryRequests);
    }

    [Theory]
    [InlineData(0)]
    [InlineData(-1)]
    public void Rejects_Nonpositive_Capacity(int capacity)
    {
        Assert.Throws<ArgumentOutOfRangeException>(() =>
            new RealtimePersistenceRepairTracker(
                TimeProvider.System,
                capacity: capacity));
    }
}
