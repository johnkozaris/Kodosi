using System.Threading.Channels;
using Kodosi.TerminalConnections;

namespace Kodosi.Push;

internal sealed partial class PushNotices(ConnectionDirectory directory, TerminalAlerts alerts, PushDelivery delivery, PushSettings settings,
    TimeProvider clock, ILogger<PushNotices> logger) : BackgroundService
{
    private const int MostQueued = 1024;
    private const int MostSending = 8;
    private static readonly TimeSpan Tick = TimeSpan.FromSeconds(1);
    private readonly Channel<PushNotice> queued = Channel.CreateBounded<PushNotice>(new BoundedChannelOptions(MostQueued) { SingleWriter = true });

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        if (!settings.Sends) return;
        directory.Reported += alerts.Observe;
        try
        {
            await Task.WhenAll(QueueAsync(stoppingToken), SendAsync(stoppingToken));
        }
        catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
        {
        }
        finally
        {
            directory.Reported -= alerts.Observe;
        }
    }

    private async Task QueueAsync(CancellationToken ct)
    {
        using var timer = new PeriodicTimer(Tick, clock);
        while (await timer.WaitForNextTickAsync(ct))
            foreach (var notice in alerts.TakeDue())
                if (!queued.Writer.TryWrite(notice)) Dropped(logger);
    }

    private Task SendAsync(CancellationToken ct) =>
        Parallel.ForEachAsync(queued.Reader.ReadAllAsync(ct), new ParallelOptions { MaxDegreeOfParallelism = MostSending, CancellationToken = ct },
            async (notice, token) =>
            {
                try
                {
                    await delivery.DeliverAsync(notice, token);
                }
                catch (Exception error) when (!token.IsCancellationRequested)
                {
                    if (Failures.IsDatabaseUnavailable(error)) Unreachable(logger, Failures.Describe(error));
                    else Failed(logger, error);
                }
            });

    [LoggerMessage(Level = LogLevel.Warning, Message = "A push notification was dropped, because too many wait to be sent")]
    private static partial void Dropped(ILogger logger);

    [LoggerMessage(Level = LogLevel.Warning, Message = "A push notification was dropped, because the database was not reachable: {Failure}")]
    private static partial void Unreachable(ILogger logger, string failure);

    [LoggerMessage(Level = LogLevel.Error, Message = "A push notification failed")]
    private static partial void Failed(ILogger logger, Exception error);
}
