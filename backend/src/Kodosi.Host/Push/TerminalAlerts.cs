using Kodosi.TerminalConnections;

namespace Kodosi.Push;

internal sealed record PushNotice(Guid TerminalId, string ComputerId, Guid OwnerId, TerminalStatus Status, int Blocked);

internal sealed partial class TerminalAlerts(ConnectionDirectory directory, TimeProvider clock, ILogger<TerminalAlerts> logger)
{
    internal static readonly TimeSpan Delay = TimeSpan.FromSeconds(15);
    internal static readonly TimeSpan Hour = TimeSpan.FromHours(1);
    internal const int MostEachHour = 120;
    private readonly Lock sync = new();
    private readonly Dictionary<Guid, Alert> waiting = [];
    private readonly Dictionary<Guid, Queue<DateTimeOffset>> sent = [];

    public void Observe(TerminalReport report)
    {
        var state = report.After?.State;
        lock (sync)
        {
            if (waiting.TryGetValue(report.SessionId, out var alert))
            {
                if (alert.State == state) return;
                waiting.Remove(report.SessionId);
            }
            if (report.First || state is not ("blocked" or "done" or "error") || report.Before?.State == state
                || waiting.Count >= ConnectionDirectory.MaximumHostedTerminals)
                return;
            waiting[report.SessionId] = new Alert(report.OwnerId, report.HostDeviceId, state, clock.GetUtcNow() + Delay);
        }
    }

    public List<PushNotice> TakeDue()
    {
        var now = clock.GetUtcNow();
        List<KeyValuePair<Guid, Alert>> due;
        lock (sync)
        {
            due = waiting.Where(x => x.Value.Due <= now).ToList();
            foreach (var (terminalId, _) in due) waiting.Remove(terminalId);
        }
        List<PushNotice> notices = [];
        foreach (var (terminalId, alert) in due)
        {
            if (directory.Live(terminalId) is not (true, { } status) || status.State != alert.State) continue;
            if (!Allow(alert.OwnerId, now))
            {
                Capped(logger, alert.OwnerId, MostEachHour);
                continue;
            }
            notices.Add(new PushNotice(terminalId, alert.ComputerId, alert.OwnerId, status, directory.Blocked(alert.OwnerId)));
        }
        Forget(now);
        return notices;
    }

    private bool Allow(Guid ownerId, DateTimeOffset now)
    {
        lock (sync)
        {
            if (!sent.TryGetValue(ownerId, out var times)) sent[ownerId] = times = new Queue<DateTimeOffset>();
            while (times.TryPeek(out var oldest) && now - oldest >= Hour) times.Dequeue();
            if (times.Count >= MostEachHour) return false;
            times.Enqueue(now);
            return true;
        }
    }

    private void Forget(DateTimeOffset now)
    {
        lock (sync)
            foreach (var (ownerId, times) in sent.ToArray())
            {
                while (times.TryPeek(out var oldest) && now - oldest >= Hour) times.Dequeue();
                if (times.Count == 0) sent.Remove(ownerId);
            }
    }

    private sealed record Alert(Guid OwnerId, string ComputerId, string State, DateTimeOffset Due);

    [LoggerMessage(Level = LogLevel.Information, Message = "A push notification of account {Account} was dropped, because it had {Most} in the last hour")]
    private static partial void Capped(ILogger logger, Guid account, int most);
}
