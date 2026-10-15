namespace Kodosi;

/// <summary>
/// Background work that runs once in each period. A failed pass is logged and the next pass runs, so that a
/// fault in one pass never stops the server.
/// </summary>
internal abstract partial class PeriodicWork(TimeSpan period, TimeProvider clock, ILogger logger, bool startNow = false) : BackgroundService
{
    protected TimeProvider Clock => clock;

    protected virtual bool Enabled => true;

    /// <summary>A failure that a later pass can overcome, such as an unreachable database. It is a warning, not an error.</summary>
    protected virtual bool Waits(Exception error) => Failures.IsDatabaseUnavailable(error);

    protected internal abstract Task RunAsync(CancellationToken ct);

    protected sealed override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        if (!Enabled) return;
        using var timer = new PeriodicTimer(period, clock);
        try
        {
            if (startNow) await PassAsync(stoppingToken);
            while (await timer.WaitForNextTickAsync(stoppingToken)) await PassAsync(stoppingToken);
        }
        catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
        {
        }
    }

    internal async Task PassAsync(CancellationToken ct)
    {
        try
        {
            await RunAsync(ct);
        }
        catch (Exception error) when (!ct.IsCancellationRequested)
        {
            if (Waits(error)) CouldNotFinish(logger, GetType().Name, Failures.Describe(error));
            else Failed(logger, error, GetType().Name);
        }
    }

    [LoggerMessage(Level = LogLevel.Warning, Message = "{Work} could not finish and runs again after its period: {Failure}")]
    private static partial void CouldNotFinish(ILogger logger, string work, string failure);

    [LoggerMessage(Level = LogLevel.Error, Message = "{Work} failed and runs again after its period")]
    private static partial void Failed(ILogger logger, Exception error, string work);
}
