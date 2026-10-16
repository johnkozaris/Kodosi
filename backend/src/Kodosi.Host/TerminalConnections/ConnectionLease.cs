using Kodosi.Data;
using Npgsql;

namespace Kodosi.TerminalConnections;

/// <summary>
/// The lease that lets only one process serve: a PostgreSQL advisory lock on a connection of its own. PostgreSQL ends
/// a lease session that stays silent for <see cref="HolderSilence"/>, so that a hung or cut-off holder cannot block the
/// next start. A lost lease stops this process.
/// </summary>
public sealed partial class ConnectionLease(DatabaseConnection database, ConnectionDirectory connections, IHostApplicationLifetime lifetime,
    TimeProvider clock, ILogger<ConnectionLease> logger) : IHostedService, IAsyncDisposable
{
    private const long Key = 0x4B4F444F5349;
    internal static readonly TimeSpan HolderSilence = TimeSpan.FromSeconds(30);
    private static readonly TimeSpan Probe = TimeSpan.FromSeconds(5);
    private enum Attempt { Held, Other, Unreachable }
    private readonly CancellationTokenSource stopping = new();
    private readonly Lock stopSync = new();
    private NpgsqlConnection? connection;
    private Task? monitor;
    private Task? stopped;
    private int disposed;

    /// <summary>How long a start waits for an earlier holder: longer than PostgreSQL needs to end a silent holder.</summary>
    internal TimeSpan AcquireWait { get; init; } = HolderSilence + TimeSpan.FromSeconds(15);

    public bool Held => connection is not null && monitor is not { IsCompleted: true };

    public async Task AcquireAsync(CancellationToken ct)
    {
        if (connection is not null) return;
        var started = clock.GetTimestamp();
        var told = false;
        while (true)
        {
            var attempt = await TakeAsync(ct);
            if (attempt == Attempt.Held) return;
            if (clock.GetElapsedTime(started) >= AcquireWait)
                throw new InvalidOperationException(attempt == Attempt.Other
                    ? "Another Kodosi backend owns the terminal connection lease."
                    : "The database for the terminal connection lease is not reachable.");
            if (attempt == Attempt.Other && !told) { WaitsForHolder(logger); told = true; }
            await Task.Delay(TimeSpan.FromSeconds(1), clock, ct);
        }
    }

    public async Task StartAsync(CancellationToken ct)
    {
        await AcquireAsync(ct);
        lifetime.ApplicationStopping.Register(connections.StopAll);
        monitor = MonitorAsync(stopping.Token);
    }

    private async Task<Attempt> TakeAsync(CancellationToken ct)
    {
        NpgsqlConnection? candidate = null;
        try
        {
            using var limit = CancellationTokenSource.CreateLinkedTokenSource(ct);
            limit.CancelAfter(TimeSpan.FromSeconds(4));
            candidate = new NpgsqlConnection(database.LeaseConnectionString);
            await candidate.OpenAsync(limit.Token);
            await using (var silence = new NpgsqlCommand("SET idle_session_timeout = '30s'", candidate) { CommandTimeout = 3 })
                await silence.ExecuteNonQueryAsync(limit.Token);
            await using var command = new NpgsqlCommand("SELECT pg_try_advisory_lock(@key)", candidate) { CommandTimeout = 3 };
            command.Parameters.AddWithValue("key", Key);
            if (await command.ExecuteScalarAsync(limit.Token) is not true) return Attempt.Other;
            connection = candidate; candidate = null;
            return Attempt.Held;
        }
        catch (Exception) when (!ct.IsCancellationRequested)
        {
            return Attempt.Unreachable;
        }
        finally
        {
            if (candidate is not null) await candidate.DisposeAsync();
        }
    }

    private async Task<bool> HeldAsync(CancellationToken ct)
    {
        if (connection is null) return false;
        try
        {
            using var probe = CancellationTokenSource.CreateLinkedTokenSource(ct);
            probe.CancelAfter(TimeSpan.FromSeconds(3));
            await using var command = new NpgsqlCommand("SELECT 1", connection) { CommandTimeout = 3 };
            await command.ExecuteScalarAsync(probe.Token).WaitAsync(TimeSpan.FromSeconds(4), clock, ct);
            return true;
        }
        catch (Exception) when (!ct.IsCancellationRequested)
        {
            return false;
        }
    }

    private async Task MonitorAsync(CancellationToken ct)
    {
        try
        {
            using var timer = new PeriodicTimer(Probe, clock);
            while (await timer.WaitForNextTickAsync(ct))
            {
                if (await HeldAsync(ct)) continue;
                connections.StopAll();
                LeaseLost(logger);
                lifetime.StopApplication();
                return;
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
        }
    }

    public Task StopAsync(CancellationToken ct)
    {
        lock (stopSync) return stopped ??= StopOnceAsync();
    }

    private async Task StopOnceAsync()
    {
        connections.StopAll();
        await stopping.CancelAsync();
        if (monitor is not null) await monitor;
        if (connection is not null)
        {
            await connection.DisposeAsync();
            connection = null;
        }
    }

    public async ValueTask DisposeAsync()
    {
        if (Interlocked.Exchange(ref disposed, 1) != 0) return;
        await StopAsync(CancellationToken.None);
        stopping.Dispose();
    }

    [LoggerMessage(Level = LogLevel.Warning, Message = "Another Kodosi backend holds the terminal connection lease; this start waits for it.")]
    private static partial void WaitsForHolder(ILogger logger);

    [LoggerMessage(Level = LogLevel.Critical, Message = "Terminal connection lease was lost; stopping the backend.")]
    private static partial void LeaseLost(ILogger logger);
}
