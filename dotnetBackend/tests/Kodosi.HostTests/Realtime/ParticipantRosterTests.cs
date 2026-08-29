using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class ParticipantRosterTests
{
    [Fact]
    public void TryAddSharedParticipant_Rejects_New_Participant_When_At_Capacity()
    {
        var roster = new ParticipantRoster();
        roster.TryAddSharedParticipant("viewer-1", int.MaxValue, out _);
        roster.TryAddSharedParticipant("viewer-2", int.MaxValue, out _);

        var added = roster.TryAddSharedParticipant("viewer-3", 2, out var transition);

        Assert.False(added);
        Assert.Equal(2, roster.GetStreamDemand().SharedParticipantCount);
        Assert.False(transition.Changed);
    }

    [Fact]
    public void TryAddSharedParticipant_Allows_Refreshing_Existing_Participant_At_Capacity()
    {
        var roster = new ParticipantRoster();
        roster.TryAddSharedParticipant("viewer-1", int.MaxValue, out _);
        roster.TryAddSharedParticipant("viewer-2", int.MaxValue, out _);

        var added = roster.TryAddSharedParticipant("viewer-2", 2, out var transition);

        Assert.True(added);
        Assert.Equal(2, roster.GetStreamDemand().SharedParticipantCount);
        Assert.False(transition.Changed);
    }

    [Fact]
    public void GetStaleSharedParticipantConnectionIds_Returns_Only_Expired_Shared_Viewers()
    {
        var clock = new TestTimeProvider(new DateTimeOffset(2026, 4, 18, 12, 0, 0, TimeSpan.Zero));
        var roster = new ParticipantRoster(clock);
        roster.TryAddSharedParticipant("viewer-stale", int.MaxValue, out _);
        clock.Advance(TimeSpan.FromMinutes(2));
        roster.TryAddSharedParticipant("viewer-fresh", int.MaxValue, out _);

        var staleConnectionIds = roster.GetStaleSharedParticipantConnectionIds(TimeSpan.FromMinutes(1));

        Assert.Equal(["viewer-stale"], staleConnectionIds);
    }

    [Fact]
    public void GetStaleOwnerParticipantConnectionIds_Returns_Only_Expired_Owner_Viewers()
    {
        var clock = new TestTimeProvider(new DateTimeOffset(2026, 4, 18, 12, 0, 0, TimeSpan.Zero));
        var roster = new ParticipantRoster(clock);
        roster.AddOwnerParticipant("owner-stale");
        clock.Advance(TimeSpan.FromMinutes(2));
        roster.AddOwnerParticipant("owner-fresh");

        var staleConnectionIds = roster.GetStaleOwnerParticipantConnectionIds(TimeSpan.FromMinutes(1));

        Assert.Equal(["owner-stale"], staleConnectionIds);
    }
}
