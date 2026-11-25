using System.Threading.Channels;

namespace Kodosi.Push;

internal sealed partial class PushNotices(TerminalAlerts alerts, PushDelivery delivery, PushRecords records, PushSettings settings,
    TimeProvider clock, ILogger<PushNotices> logger) : BackgroundService
{
    private const int MostQueued = 1024;
    private const int MostSending = 8;
    private static readonly TimeSpan Tick = TimeSpan.FromSeconds(1);
    private static readonly TimeSpan SaveRetry = TimeSpan.FromSeconds(10);
    private static readonly TimeSpan LastSave = TimeSpan.FromSeconds(5);
    private readonly Channel<PushNotice>[] lanes = [.. Enumerable.Range(0, MostSending).Select(_ => Channel.CreateBounded<PushNotice>(
        new BoundedChannelOptions(MostQueued / MostSending) { SingleReader = true, SingleWriter = true }))];

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        if (!settings.Sends) return;
        alerts.Follow();
        try
        {
            alerts.Restore(await records.LoadAsync(stoppingToken));
            await Task.WhenAll([QueueAsync(stoppingToken), KeepAsync(stoppingToken), .. lanes.Select(lane => SendAsync(lane, stoppingToken))]);
        }
        catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
        {
        }
        finally
        {
            alerts.Unfollow();
            using var stopping = new CancellationTokenSource(LastSave, clock);
            try
            {
                await SaveAsync(stopping.Token);
            }
            catch (OperationCanceledException) when (stopping.IsCancellationRequested)
            {
            }
        }
    }

    private async Task KeepAsync(CancellationToken ct)
    {
        using var timer = new PeriodicTimer(Tick, clock);
        while (await timer.WaitForNextTickAsync(ct))
            if (!await SaveAsync(ct)) await Task.Delay(SaveRetry, clock, ct);
    }

    private async Task<bool> SaveAsync(CancellationToken ct)
    {
        var unsaved = alerts.TakeUnsaved();
        if (unsaved.Count == 0) return true;
        var saved = false;
        try
        {
            saved = await records.SaveAsync(unsaved, ct);
        }
        finally
        {
            if (!saved) alerts.SaveLater(unsaved);
        }
        return saved;
    }

    private async Task QueueAsync(CancellationToken ct)
    {
        using var timer = new PeriodicTimer(Tick, clock);
        while (await timer.WaitForNextTickAsync(ct))
        {
            foreach (var notice in alerts.Take()) Queue(notice);
            foreach (var (ownerId, deviceId) in delivery.Released()) Queue(new PushNotice(ownerId, alerts.PushedOf(ownerId), DeviceId: deviceId));
        }
    }

    // Shard by OwnerId so each user's notices are delivered in order and the newest snapshot
    // arrives last.
    private void Queue(PushNotice notice)
    {
        if (!lanes[(uint)notice.OwnerId.GetHashCode() % MostSending].Writer.TryWrite(notice)) Dropped(logger);
    }

    private async Task SendAsync(Channel<PushNotice> lane, CancellationToken ct)
    {
        await foreach (var notice in lane.Reader.ReadAllAsync(ct))
            try
            {
                await delivery.DeliverAsync(notice, ct);
            }
            catch (Exception error) when (!ct.IsCancellationRequested)
            {
                if (Failures.IsDatabaseUnavailable(error)) Unreachable(logger, Failures.Describe(error));
                else Failed(logger, error);
            }
    }

    [LoggerMessage(Level = LogLevel.Warning, Message = "A push notification was dropped, because too many wait to be sent")]
    private static partial void Dropped(ILogger logger);

    [LoggerMessage(Level = LogLevel.Warning, Message = "A push notification was dropped, because the database was not reachable: {Failure}")]
    private static partial void Unreachable(ILogger logger, string failure);

    [LoggerMessage(Level = LogLevel.Error, Message = "A push notification failed")]
    private static partial void Failed(ILogger logger, Exception error);
}
