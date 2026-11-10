using Kodosi.Data;
using Kodosi.Push;
using Kodosi.TerminalConnections;
using Microsoft.Extensions.Logging.Abstractions;
using Xunit;

namespace Kodosi.HostTests;

public sealed class TerminalAlertTests
{
    private readonly ConnectionDirectory directory = new();
    private readonly ManualClock clock = new();
    private readonly TerminalAlerts alerts;
    private readonly Guid owner = Guid.CreateVersion7();

    public TerminalAlertTests()
    {
        alerts = new TerminalAlerts(directory, clock, NullLogger<TerminalAlerts>.Instance);
        directory.Reported += alerts.Observe;
    }

    private DeviceConnection Connect()
    {
        var host = new DeviceConnection(new FakeSocket(), owner, "computer", directory.Relays, TimeProvider.System, hosts: true);
        directory.Register(host);
        return host;
    }

    private Session Hosted(DeviceConnection host)
    {
        var terminal = new Session { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7() };
        directory.Host(host, terminal);
        return terminal;
    }

    private void Report(DeviceConnection host, Session terminal, string state, string? kind = null) =>
        directory.Share(host, terminal.Id, terminal.IncarnationId, new TerminalStatus("claude", state, kind, 1));

    [Fact]
    public async Task AChangeIntoWaitingIsSentFifteenSecondsLaterWithTheNewestStatus()
    {
        await using var host = Connect();
        var terminal = Hosted(host); var other = Hosted(host);
        Report(host, other, "blocked");
        Report(host, terminal, "working");
        Report(host, terminal, "blocked", "permission");
        clock.Advance(TerminalAlerts.Delay - TimeSpan.FromSeconds(1));
        Assert.Empty(alerts.TakeDue());
        Report(host, terminal, "blocked", "question");
        clock.Advance(TimeSpan.FromSeconds(1));
        var notice = Assert.Single(alerts.TakeDue());
        Assert.Equal((terminal.Id, owner, 2), (notice.TerminalId, notice.OwnerId, notice.Blocked));
        Assert.Equal(new TerminalStatus("claude", "blocked", "question", 1), notice.Status);
        clock.Advance(TerminalAlerts.Delay);
        Assert.Empty(alerts.TakeDue());
    }

    [Fact]
    public async Task OnlyTheNewestStateCountsAndAChangeAwayOrAnEndDropsIt()
    {
        await using var host = Connect();
        var terminal = Hosted(host);
        Report(host, terminal, "working");
        Report(host, terminal, "blocked");
        clock.Advance(TimeSpan.FromSeconds(10));
        Report(host, terminal, "done");
        clock.Advance(TimeSpan.FromSeconds(5));
        Assert.Empty(alerts.TakeDue());
        clock.Advance(TimeSpan.FromSeconds(10));
        Assert.Equal("done", Assert.Single(alerts.TakeDue()).Status.State);

        Report(host, terminal, "error");
        clock.Advance(TimeSpan.FromSeconds(5));
        Report(host, terminal, "working");
        clock.Advance(TerminalAlerts.Delay);
        Assert.Empty(alerts.TakeDue());

        Report(host, terminal, "blocked");
        directory.Unhost(host, terminal.Id);
        clock.Advance(TerminalAlerts.Delay);
        Assert.Empty(alerts.TakeDue());
    }

    [Fact]
    public async Task TheFirstStatusThatAHostReportsAfterItConnectsSendsNothing()
    {
        var first = Connect();
        var terminal = Hosted(first);
        Report(first, terminal, "blocked");
        clock.Advance(TerminalAlerts.Delay);
        Assert.Empty(alerts.TakeDue());

        directory.Remove(first);
        await first.DisposeAsync();
        await using var again = Connect();
        directory.Host(again, terminal);
        Report(again, terminal, "error");
        clock.Advance(TerminalAlerts.Delay);
        Assert.Empty(alerts.TakeDue());
        Report(again, terminal, "working");
        Report(again, terminal, "error");
        clock.Advance(TerminalAlerts.Delay);
        Assert.Equal("error", Assert.Single(alerts.TakeDue()).Status.State);
    }

    [Fact]
    public async Task APersonGetsAtMostAHundredAndTwentyPushesAnHour()
    {
        await using var host = Connect();
        var terminal = Hosted(host);
        Report(host, terminal, "working");
        int Wait()
        {
            Report(host, terminal, "blocked");
            clock.Advance(TerminalAlerts.Delay);
            var sent = alerts.TakeDue().Count;
            Report(host, terminal, "working");
            return sent;
        }
        Assert.Equal(TerminalAlerts.MostEachHour, Enumerable.Range(0, TerminalAlerts.MostEachHour).Sum(_ => Wait()));
        Assert.Equal(0, Wait());
        clock.Advance(TerminalAlerts.Hour);
        Assert.Equal(1, Wait());
    }
}
