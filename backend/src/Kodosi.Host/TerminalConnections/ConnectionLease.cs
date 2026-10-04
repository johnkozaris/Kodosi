using Npgsql;

namespace Kodosi.TerminalConnections;

public sealed class ConnectionLease(NpgsqlDataSource source, ConnectionDirectory connections, IHostApplicationLifetime lifetime,
    ILogger<ConnectionLease> logger) : IHostedService, IAsyncDisposable
{
    private const long Key = 0x4B4F444F5349;
    private static readonly TimeSpan Retake = TimeSpan.FromSeconds(30);
    private enum Attempt { Held, Other, Unreachable }
    private NpgsqlConnection? connection;
    private readonly CancellationTokenSource stopping = new();
    private Task? monitor;
    private int disposed;

    public async Task AcquireAsync(CancellationToken ct)
    {
        if (connection is not null) return;
        switch (await TakeAsync(ct))
        {
            case Attempt.Other: throw new InvalidOperationException("Another Kodosi backend owns the terminal connection lease.");
            case Attempt.Unreachable: throw new InvalidOperationException("The database for the terminal connection lease is not reachable.");
        }
    }
    public async Task StartAsync(CancellationToken ct)
    {
        await AcquireAsync(ct);
        monitor = MonitorAsync(stopping.Token);
    }
    private async Task<Attempt> TakeAsync(CancellationToken ct)
    {
        NpgsqlConnection? candidate = null;
        try
        {
            using var limit = CancellationTokenSource.CreateLinkedTokenSource(ct);
            limit.CancelAfter(TimeSpan.FromSeconds(4));
            candidate = await source.OpenConnectionAsync(limit.Token);
            await using var command = new NpgsqlCommand("SELECT pg_try_advisory_lock(@key)", candidate) { CommandTimeout = 3 };
            command.Parameters.AddWithValue("key", Key);
            if (await command.ExecuteScalarAsync(limit.Token) is not true) return Attempt.Other;
            connection = candidate; candidate = null;
            return Attempt.Held;
        }
        catch (Exception) when (!ct.IsCancellationRequested) { return Attempt.Unreachable; }
        finally { if (candidate is not null) await candidate.DisposeAsync(); }
    }
    private async Task<bool> HeldAsync(CancellationToken ct)
    {
        if (connection is null) return false;
        try
        {
            using var probe = CancellationTokenSource.CreateLinkedTokenSource(ct);
            probe.CancelAfter(TimeSpan.FromSeconds(3));
            await using var command = new NpgsqlCommand("SELECT 1", connection) { CommandTimeout = 3 };
            await command.ExecuteScalarAsync(probe.Token).WaitAsync(TimeSpan.FromSeconds(4), ct);
            return true;
        }
        catch (Exception) when (!ct.IsCancellationRequested) { return false; }
    }
    private async Task MonitorAsync(CancellationToken ct)
    {
        try
        {
            using var timer = new PeriodicTimer(TimeSpan.FromSeconds(5));
            while (await timer.WaitForNextTickAsync(ct))
            {
                if (await HeldAsync(ct)) continue;
                if (connection is { } broken)
                {
                    connection = null;
                    _ = broken.DisposeAsync().AsTask().ContinueWith(static _ => { }, TaskScheduler.Default);
                }
                var lost = TimeProvider.System.GetTimestamp();
                var attempt = await TakeAsync(ct);
                while (attempt != Attempt.Held && TimeProvider.System.GetElapsedTime(lost) < Retake)
                {
                    await Task.Delay(TimeSpan.FromSeconds(1), ct);
                    attempt = await TakeAsync(ct);
                }
                if (attempt == Attempt.Held)
                {
                    logger.LogWarning("Terminal connection lease was lost and taken again after {Seconds:0} s.", TimeProvider.System.GetElapsedTime(lost).TotalSeconds);
                    continue;
                }
                connections.StopAll();
                logger.LogCritical("Terminal connection lease was lost ({Attempt}); stopping the backend.", attempt);
                lifetime.StopApplication();
                return;
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested) { }
    }
    public async Task StopAsync(CancellationToken ct)
    {
        connections.StopAll(); await stopping.CancelAsync();
        if (monitor is not null) await monitor;
        if (connection is not null) { await connection.DisposeAsync(); connection = null; }
    }
    public async ValueTask DisposeAsync()
    {
        if (Interlocked.Exchange(ref disposed, 1) != 0) return;
        await StopAsync(CancellationToken.None); stopping.Dispose();
    }
}
