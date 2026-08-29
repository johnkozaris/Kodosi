using System.Diagnostics;
using Kodosi.Application;
using Microsoft.Extensions.Options;
using Kodosi.Host.Observability;
using Kodosi.Host.Realtime;

namespace Kodosi.Host;

internal sealed class GracefulShutdownHostedService(
    SessionBroadcaster broadcaster,
    IConnectionRegistry connections,
    UserEventBroadcaster userEvents,
    IOptions<HostOptions> hostOptions,
    OperationalMetrics metrics,
    IHostApplicationLifetime lifetime,
    ILogger<GracefulShutdownHostedService> logger) : IHostedService, IDisposable
{
    private const int BatchSize = 50;
    private static readonly TimeSpan BatchDelay = TimeSpan.FromMilliseconds(100);
    private static readonly TimeSpan DrainTimeoutCeiling = TimeSpan.FromSeconds(30);

    private static readonly TimeSpan HostingShutdownBuffer = TimeSpan.FromSeconds(1);

    private readonly SessionBroadcaster _broadcaster = broadcaster;
    private readonly IConnectionRegistry _connections = connections;
    private readonly UserEventBroadcaster _userEvents = userEvents;
    private readonly IOptions<HostOptions> _hostOptions = hostOptions;
    private readonly OperationalMetrics _metrics = metrics;
    private readonly IHostApplicationLifetime _lifetime = lifetime;
    private readonly ILogger<GracefulShutdownHostedService> _logger = logger;

    private readonly Lock _drainLock = new();
    private IDisposable? _stoppingRegistration;
    private Task? _drain;
    private volatile bool _draining;

    public bool IsDraining => _draining;

    public Task StartAsync(CancellationToken cancellationToken)
    {










        _stoppingRegistration = _lifetime.ApplicationStopping.Register(
            () => _ = EnsureDrainStarted());
        return Task.CompletedTask;
    }

    public async Task StopAsync(CancellationToken cancellationToken)
    {




        var drain = EnsureDrainStarted();
        try
        {
            await drain.WaitAsync(cancellationToken);
        }
        catch (OperationCanceledException)
        {


        }
    }

    public void Dispose() => _stoppingRegistration?.Dispose();

    private Task EnsureDrainStarted()
    {
        lock (_drainLock)
        {
            return _drain ??= Task.Run(() => DrainAsync(CancellationToken.None));
        }
    }

    private async Task DrainAsync(CancellationToken cancellationToken)
    {
        _draining = true;
        _metrics.RecordDrainStarted();
        var drainStopwatch = Stopwatch.StartNew();
        _logger.LogInformation("Graceful shutdown: draining WebSocket connections");

        var hostQueueCount = _broadcaster.CompleteAllHostQueues(CloseReason.ServerRestarting);
        var allClientQueues = _broadcaster.GetAllDownstreamQueues();
        var userEventQueueCount = _userEvents.CompleteAll(CloseReason.ServerRestarting);
        _logger.LogInformation(
            "Draining {HostCount} host, {ParticipantCount} participant, and {UserEventCount} user-event connections in batches of {Batch}",
            hostQueueCount,
            allClientQueues.Count,
            userEventQueueCount,
            BatchSize);

        if (hostQueueCount == 0
            && allClientQueues.Count == 0
            && userEventQueueCount == 0)
        {
            _logger.LogInformation("Graceful shutdown: no connections to drain; skipping wait");
        }
        else
        {
            for (var i = 0; i < allClientQueues.Count; i += BatchSize)
            {
                var batch = allClientQueues.Skip(i).Take(BatchSize);
                foreach (var queue in batch)
                {
                    queue.Complete(QueueCompletionCause.Drain, discardPending: true);
                }

                if (i + BatchSize < allClientQueues.Count)
                {
                    await Task.Delay(BatchDelay, CancellationToken.None);
                }
            }


            var hostingTimeout = _hostOptions.Value.ShutdownTimeout;
            var drainBudget = hostingTimeout - HostingShutdownBuffer;
            var drainDelay = drainBudget > TimeSpan.Zero && drainBudget < DrainTimeoutCeiling
                ? drainBudget
                : DrainTimeoutCeiling;
            _logger.LogInformation(
                "Graceful shutdown: draining for {DrainDelay} (hosting timeout {HostingTimeout})",
                drainDelay,
                hostingTimeout);

            var deadline = DateTimeOffset.UtcNow + drainDelay;
            try
            {
                while (DateTimeOffset.UtcNow < deadline
                    && (_connections.ActiveConnectionCount > 0
                        || _userEvents.ActiveConnectionCount > 0))
                {
                    await Task.Delay(BatchDelay, cancellationToken);
                }
            }
            catch (OperationCanceledException)
            {
            }
        }

        drainStopwatch.Stop();
        _metrics.RecordDrainDuration(drainStopwatch.Elapsed.TotalMilliseconds);
        _metrics.RecordDrainCompleted();
        _logger.LogInformation(
            "Graceful shutdown: drain complete in {DurationMs}ms",
            drainStopwatch.Elapsed.TotalMilliseconds);
    }
}
