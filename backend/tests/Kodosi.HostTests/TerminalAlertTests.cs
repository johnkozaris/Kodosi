using Kodosi.Data;
using Kodosi.Push;
using Kodosi.TerminalConnections;
using Microsoft.Extensions.Logging.Abstractions;
using Xunit;

namespace Kodosi.HostTests;

public sealed class TerminalAlertTests
{
    private readonly ManualClock clock = new();
    private ConnectionDirectory directory;
    private TerminalAlerts alerts;
    private readonly Guid owner = Guid.CreateVersion7();

    public TerminalAlertTests()
    {
        directory = new ConnectionDirectory(clock);
        alerts = new TerminalAlerts(directory, clock, NullLogger<TerminalAlerts>.Instance);
        alerts.Follow();
    }

    private DeviceConnection Connect(string device, bool hosts = false)
    {
        var connection = new DeviceConnection(new FakeSocket(), owner, device, directory.Relays, clock, hosts);
        directory.Register(connection);
        directory.Ready(connection);
        return connection;
    }

    private Session Hosted(DeviceConnection host, Session? terminal = null)
    {
        terminal ??= new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7() };
        directory.Host(host, terminal);
        return terminal;
    }

    private void Report(DeviceConnection host, Session terminal, string state, bool attention = true, string? kind = null) =>
        directory.Share(host, terminal.Id, terminal.IncarnationId, new TerminalStatus("claude", state, kind, 1, attention ? true : null));

    private List<PushNotice> After(TimeSpan time)
    {
        clock.Advance(time);
        return alerts.Take();
    }

    private static PushAlert Alert(List<PushNotice> notices) => Assert.Single(notices).Alert!;

    private List<PushRecord> Restart()
    {
        var saved = alerts.TakeUnsaved().Values.OfType<PushRecord>().ToList();
        alerts.Unfollow();
        directory = new ConnectionDirectory(clock);
        alerts = new TerminalAlerts(directory, clock, NullLogger<TerminalAlerts>.Instance);
        alerts.Follow();
        return saved;
    }

    private static Guid[] Withdrawn(List<PushNotice> notices)
    {
        var notice = Assert.Single(notices);
        Assert.Null(notice.Alert);
        return [.. notice.Pushed.Select(x => x.TerminalId)];
    }

    [Fact]
    public async Task AStatusThatTheOwnerDidNotSeeIsPushedThirtySecondsLaterOnlyWhileTheOwnerIsAway()
    {
        await using var computer = Connect("computer", hosts: true);
        var terminal = Hosted(computer);
        directory.Present(computer, Presence.Kodosi);
        Report(computer, terminal, "done");
        Assert.Empty(After(TerminalAlerts.Delay));
        directory.Present(computer, Presence.Here);
        Report(computer, terminal, "error");
        Assert.Empty(After(TerminalAlerts.Delay));
        directory.Present(computer, Presence.Away);
        Report(computer, terminal, "done");
        clock.Advance(TerminalAlerts.Delay - TimeSpan.FromSeconds(1));
        directory.Present(computer, Presence.Here);
        Assert.Empty(After(TimeSpan.FromSeconds(1)));
        directory.Present(computer, Presence.Away);
        Assert.Empty(After(TerminalAlerts.Delay));

        Report(computer, terminal, "blocked", kind: "permission");
        Assert.Empty(After(TerminalAlerts.Delay - TimeSpan.FromSeconds(1)));
        Report(computer, terminal, "blocked", kind: "question");
        Assert.Empty(After(TimeSpan.FromSeconds(1)));
        var notice = Assert.Single(After(TerminalAlerts.Delay));
        Assert.Equal(owner, notice.OwnerId);
        Assert.Equal(new PushAlert(terminal.Id, new TerminalStatus("claude", "blocked", "question", 1, true), Sound: true, Waits: 1), notice.Alert);
        Assert.Equal([terminal.Id], notice.Pushed.Select(x => x.TerminalId));
        Assert.Empty(After(TerminalAlerts.Delay));
    }

    [Fact]
    public async Task AWaitThatTheOwnerDidNotSeeIsPushedThirtySecondsAfterTheOwnerLeavesUnlessTheOwnerComesBack()
    {
        await using var computer = Connect("computer", hosts: true);
        var waiting = Hosted(computer); var seen = Hosted(computer); var finished = Hosted(computer); var fresh = Hosted(computer);
        directory.Present(computer, Presence.Kodosi);
        Report(computer, waiting, "blocked", kind: "permission");
        Report(computer, seen, "blocked", attention: false);
        Report(computer, finished, "done");
        Assert.Empty(After(TerminalAlerts.Delay));
        Report(computer, fresh, "blocked");
        clock.Advance(TimeSpan.FromSeconds(20));

        directory.Present(computer, Presence.Away);
        Assert.Empty(alerts.Take());
        Assert.Equal(fresh.Id, Alert(After(TerminalAlerts.Delay - TimeSpan.FromSeconds(20))).TerminalId);
        Assert.Empty(After(TimeSpan.FromSeconds(19)));
        Assert.Equal(new PushAlert(waiting.Id, new TerminalStatus("claude", "blocked", "permission", 1, true), Sound: false, Waits: 2),
            Alert(After(TimeSpan.FromSeconds(1))));

        var other = Hosted(computer);
        directory.Present(computer, Presence.Kodosi);
        Report(computer, other, "blocked");
        Assert.Empty(After(TerminalAlerts.Delay));
        directory.Present(computer, Presence.Away);
        clock.Advance(TerminalAlerts.Delay - TimeSpan.FromSeconds(1));
        directory.Present(computer, Presence.Here);
        Assert.Empty(After(TimeSpan.FromSeconds(1)));
    }

    [Fact]
    public async Task ADeviceIsAwayWhenItDoesNotRenewItsPresenceOrThirtySecondsAfterItsConnectionEnds()
    {
        await using var computer = Connect("computer", hosts: true);
        var phone = Connect("phone");
        var first = Hosted(computer);
        directory.Present(phone, Presence.Kodosi);
        Report(computer, first, "blocked");
        Assert.Empty(After(TerminalAlerts.Delay));
        clock.Advance(ConnectionDirectory.PresenceLease - TerminalAlerts.Delay - TimeSpan.FromSeconds(1));
        directory.ExpirePresence();
        clock.Advance(TimeSpan.FromSeconds(1));
        directory.ExpirePresence();
        Assert.Empty(After(TerminalAlerts.Delay - TimeSpan.FromSeconds(1)));
        Assert.Equal(first.Id, Alert(After(TimeSpan.FromSeconds(1))).TerminalId);

        directory.Present(phone, Presence.Kodosi);
        var second = Hosted(computer);
        Report(computer, second, "blocked");
        Assert.Empty(After(TerminalAlerts.Delay));
        directory.Remove(phone);
        await phone.DisposeAsync();
        clock.Advance(ConnectionDirectory.PresenceGrace - TimeSpan.FromSeconds(1));
        directory.ExpirePresence();
        clock.Advance(TimeSpan.FromSeconds(1));
        directory.ExpirePresence();
        Assert.Empty(After(TerminalAlerts.Delay - TimeSpan.FromSeconds(1)));
        var notice = Assert.Single(After(TimeSpan.FromSeconds(1)));
        Assert.Equal(second.Id, notice.Alert!.TerminalId);
        Assert.Equal([second.Id, first.Id], notice.Pushed.Select(x => x.TerminalId));
    }

    [Fact]
    public async Task ADeviceThatConnectsAgainWithinThirtySecondsKeepsItsStateUntilItsLeaseEnds()
    {
        await using var computer = Connect("computer", hosts: true);
        var phone = Connect("phone");
        var terminal = Hosted(computer);
        directory.Present(phone, Presence.Kodosi);
        var lease = clock.GetUtcNow() + ConnectionDirectory.PresenceLease;
        Report(computer, terminal, "blocked");
        Assert.Empty(After(TerminalAlerts.Delay));
        directory.Remove(phone);
        await phone.DisposeAsync();
        clock.Advance(ConnectionDirectory.PresenceGrace - TimeSpan.FromSeconds(1));
        await using var again = Connect("phone");
        clock.Advance(ConnectionDirectory.PresenceGrace);
        directory.ExpirePresence();
        clock.Advance(lease - TimeSpan.FromSeconds(1) - clock.GetUtcNow());
        directory.ExpirePresence();
        Assert.Empty(alerts.Take());

        clock.Advance(TimeSpan.FromSeconds(1));
        directory.ExpirePresence();
        Assert.Empty(After(TerminalAlerts.Delay - TimeSpan.FromSeconds(1)));
        Assert.Equal(terminal.Id, Alert(After(TimeSpan.FromSeconds(1))).TerminalId);
    }

    [Fact]
    public async Task ARemovedDeviceIsAwayAtOnceAlsoWhileItsConnectionIsLost()
    {
        await using var computer = Connect("computer", hosts: true);
        await using var phone = Connect("phone");
        var tablet = Connect("tablet");
        var terminal = Hosted(computer);
        directory.Present(phone, Presence.Kodosi);
        directory.Present(tablet, Presence.Kodosi);
        Report(computer, terminal, "blocked");
        Assert.Empty(After(TerminalAlerts.Delay));
        directory.Remove(tablet);
        await tablet.DisposeAsync();
        directory.RemoveDevice(owner, "phone");
        directory.RemoveDevice(owner, "tablet");
        Assert.Equal(terminal.Id, Alert(After(TerminalAlerts.Delay)).TerminalId);
    }

    [Fact]
    public async Task APushOlderThanADayIsForgottenOnlyWhenItsTerminalNoLongerWaitsOnAConnectedHost()
    {
        await using var computer = Connect("computer", hosts: true);
        var laptop = Connect("laptop", hosts: true);
        var waiting = Hosted(computer); var sleeping = Hosted(laptop); var later = Hosted(computer);
        Report(computer, waiting, "blocked");
        Report(laptop, sleeping, "done");
        Assert.Equal(2, After(TerminalAlerts.Delay).Count);
        directory.Remove(laptop);
        await laptop.DisposeAsync();
        Assert.Empty(After(TerminalAlerts.Remembered));
        Report(computer, later, "blocked");
        Assert.Equal([later.Id, waiting.Id], Assert.Single(After(TerminalAlerts.Delay)).Pushed.Select(x => x.TerminalId));
    }

    [Fact]
    public async Task AHostThatConnectsAgainWithTheSameStatusIsNotPushedAgain()
    {
        var computer = Connect("computer", hosts: true);
        var terminal = Hosted(computer);
        Report(computer, terminal, "blocked");
        Assert.Equal(terminal.Id, Alert(After(TerminalAlerts.Delay)).TerminalId);
        directory.Remove(computer);
        await computer.DisposeAsync();
        Assert.Empty(After(TerminalAlerts.Delay));

        await using var again = Connect("computer", hosts: true);
        Hosted(again, terminal);
        Report(again, terminal, "blocked");
        Assert.Empty(After(TerminalAlerts.Delay));
        Report(again, terminal, "done");
        Assert.Empty(Withdrawn(alerts.Take()));
        Assert.Equal("done", Alert(After(TerminalAlerts.Delay)).Status.State);
    }

    [Fact]
    public async Task APushFromBeforeARestartOfTheServerIsNotSentAgainButANewStateIs()
    {
        var computer = Connect("computer", hosts: true);
        var waiting = Hosted(computer); var seen = Hosted(computer);
        Report(computer, waiting, "blocked");
        Report(computer, seen, "done");
        Assert.Equal(2, After(TerminalAlerts.Delay).Count(notice => notice.Alert is not null));
        await computer.DisposeAsync();
        var saved = Restart();

        await using var again = Connect("computer", hosts: true);
        Hosted(again, waiting); Hosted(again, seen);
        Report(again, seen, "done", attention: false);
        alerts.Restore(saved);
        Assert.Equal([waiting.Id], Withdrawn(alerts.Take()));
        Assert.Equal([seen.Id], alerts.TakeUnsaved().Keys);
        Report(again, waiting, "blocked");
        Assert.Empty(After(TerminalAlerts.Delay));
        Report(again, waiting, "done");
        Assert.Empty(Withdrawn(alerts.Take()));
        var alert = Alert(After(TerminalAlerts.Delay));
        Assert.Equal((waiting.Id, "done"), (alert.TerminalId, alert.Status.State));
    }

    [Fact]
    public async Task AWaitThatAHostReportedBeforeThePushesCameBackGetsNoSecondPush()
    {
        var computer = Connect("computer", hosts: true);
        var terminal = Hosted(computer);
        Report(computer, terminal, "blocked");
        Alert(After(TerminalAlerts.Delay));
        await computer.DisposeAsync();
        var saved = Restart();

        await using var again = Connect("computer", hosts: true);
        Hosted(again, terminal);
        Report(again, terminal, "blocked");
        alerts.Restore(saved);
        Assert.Empty(After(ConnectionDirectory.PresenceLease));
        Assert.Empty(alerts.TakeUnsaved());
    }

    [Fact]
    public async Task ARestoredPushIsTakenBackWhenItsHostCameBackWithoutTheWaitAndKeptWhileItsHostIsAway()
    {
        var computer = Connect("computer", hosts: true); var laptop = Connect("laptop", hosts: true);
        var answered = Hosted(computer); var asleep = Hosted(laptop);
        Report(computer, answered, "blocked");
        Report(laptop, asleep, "blocked");
        Assert.Equal(2, After(TerminalAlerts.Delay).Count);
        await computer.DisposeAsync();
        await laptop.DisposeAsync();
        var saved = Restart();

        await using var again = Connect("computer", hosts: true);
        Hosted(again, answered);
        alerts.Restore(saved);
        Assert.Empty(After(ConnectionDirectory.PresenceLease - TimeSpan.FromSeconds(1)));
        Assert.Equal([asleep.Id], Withdrawn(After(TimeSpan.FromSeconds(1))));
        Assert.Equal([answered.Id], alerts.TakeUnsaved().Keys);
    }

    [Fact]
    public async Task EachPushAndEachWithdrawalWaitsToBeSavedAndTheNewerOneWins()
    {
        await using var computer = Connect("computer", hosts: true);
        var terminal = Hosted(computer);
        Report(computer, terminal, "blocked");
        Alert(After(TerminalAlerts.Delay));
        var pushed = alerts.TakeUnsaved();
        Assert.Equal(new PushRecord(owner, terminal.Id, new TerminalStatus("claude", "blocked", null, 1, true), clock.GetUtcNow()), pushed[terminal.Id]);
        Assert.Empty(alerts.TakeUnsaved());
        Report(computer, terminal, "working", attention: false);
        alerts.SaveLater(pushed);
        Assert.Null(Assert.Single(alerts.TakeUnsaved()).Value);
    }

    [Fact]
    public async Task AWithdrawalThatANewerNoticeOfTheSamePersonFollowsInTheSameSecondIsNotSent()
    {
        await using var computer = Connect("computer", hosts: true);
        var answered = Hosted(computer); var next = Hosted(computer);
        Report(computer, answered, "blocked");
        Alert(After(TerminalAlerts.Delay));
        clock.Advance(TimeSpan.FromSeconds(10));
        Report(computer, next, "blocked");
        clock.Advance(TerminalAlerts.Delay - TimeSpan.FromSeconds(1));
        Report(computer, answered, "blocked", attention: false);
        var notice = Assert.Single(After(TimeSpan.FromSeconds(1)));
        Assert.Equal(next.Id, notice.Alert?.TerminalId);
        Assert.Equal([next.Id], notice.Pushed.Select(x => x.TerminalId));
    }

    [Fact]
    public async Task APushIsWithdrawnWhenItsTerminalIsSeenChangesOrEndsButNotWhenItsHostDisconnects()
    {
        var computer = Connect("computer", hosts: true);
        var seen = Hosted(computer); var changed = Hosted(computer); var closed = Hosted(computer); var ended = Hosted(computer);
        foreach (var terminal in new[] { seen, changed, closed, ended })
        {
            Report(computer, terminal, "blocked");
            Assert.Equal(terminal.Id, Alert(After(TerminalAlerts.Delay)).TerminalId);
        }

        Report(computer, seen, "blocked", attention: false);
        Assert.Equal([ended.Id, closed.Id, changed.Id], Withdrawn(alerts.Take()));
        Report(computer, changed, "working", attention: false);
        Assert.Equal([ended.Id, closed.Id], Withdrawn(alerts.Take()));
        directory.Unhost(computer, closed.Id);
        Assert.Equal([ended.Id], Withdrawn(alerts.Take()));
        directory.Remove(computer);
        await computer.DisposeAsync();
        Assert.Empty(alerts.Take());
        directory.RemoveSession(ended.Id, owner);
        Assert.Empty(Withdrawn(alerts.Take()));
    }

    [Fact]
    public async Task OnlyAWaitHasASoundAndAPersonHearsAtMostOneEachThirtySeconds()
    {
        await using var computer = Connect("computer", hosts: true);
        var finished = Hosted(computer); var first = Hosted(computer); var second = Hosted(computer); var third = Hosted(computer);
        Report(computer, finished, "done");
        Assert.False(Alert(After(TerminalAlerts.Delay)).Sound);
        Report(computer, first, "blocked");
        clock.Advance(TerminalAlerts.Delay - TimeSpan.FromSeconds(1));
        Report(computer, second, "blocked");
        Assert.True(Alert(After(TimeSpan.FromSeconds(1))).Sound);
        Report(computer, third, "blocked");
        var quiet = TerminalAlerts.SoundSpacing - TimeSpan.FromSeconds(1);
        Assert.False(Alert(After(quiet)).Sound);
        Assert.True(Alert(After(TerminalAlerts.SoundSpacing - quiet)).Sound);
    }

    [Fact]
    public async Task APersonGetsAtMostAHundredAndTwentyPushesAnHourAndWithdrawalsDoNotCount()
    {
        await using var computer = Connect("computer", hosts: true);
        var terminals = Enumerable.Range(0, TerminalAlerts.MostEachHour + 1).Select(_ => Hosted(computer)).ToArray();
        int Wait(IEnumerable<Session> some)
        {
            foreach (var terminal in some) Report(computer, terminal, "blocked");
            var pushed = After(TerminalAlerts.Delay).Count(x => x.Alert is not null);
            foreach (var terminal in some) Report(computer, terminal, "working", attention: false);
            return pushed;
        }
        var half = TerminalAlerts.MostEachHour / 2;
        Assert.Equal(half, Wait(terminals[..half]));
        Assert.Equal(half, Wait(terminals[half..^1]));
        Assert.Equal(0, Wait(terminals[^1..]));
        clock.Advance(TerminalAlerts.Hour);
        Assert.Equal(1, Wait(terminals[^1..]));
    }
}
