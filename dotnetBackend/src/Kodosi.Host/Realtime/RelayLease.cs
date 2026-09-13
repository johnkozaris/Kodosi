using Npgsql;

namespace Kodosi.Realtime;

public sealed class RelayLease(NpgsqlDataSource source, RelayDirectory relay, IHostApplicationLifetime lifetime,
    ILogger<RelayLease> logger) : IHostedService, IAsyncDisposable
{
    private const long Key = 0x4B4F444F5349;
    private NpgsqlConnection? connection;
    private readonly CancellationTokenSource stopping = new();
    private Task? monitor;

    public async Task StartAsync(CancellationToken ct)
    {
        connection = await source.OpenConnectionAsync(ct);
        await using var command = new NpgsqlCommand("SELECT pg_try_advisory_lock(@key)", connection);
        command.Parameters.AddWithValue("key", Key);
        if (await command.ExecuteScalarAsync(ct) is not true)
        {
            await connection.DisposeAsync(); connection = null;
            throw new InvalidOperationException("Another Kodosi backend owns the relay lease.");
        }
        monitor = MonitorAsync(connection, stopping.Token);
    }
    private async Task MonitorAsync(NpgsqlConnection active, CancellationToken ct)
    {
        try
        {
            using var timer = new PeriodicTimer(TimeSpan.FromSeconds(5));
            while (await timer.WaitForNextTickAsync(ct))
            {
                using var probe = CancellationTokenSource.CreateLinkedTokenSource(ct);
                probe.CancelAfter(TimeSpan.FromSeconds(3));
                await using var command = new NpgsqlCommand("SELECT 1", active) { CommandTimeout = 3 };
                await command.ExecuteScalarAsync(probe.Token).WaitAsync(TimeSpan.FromSeconds(4), ct);
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested) { }
        catch (Exception error) when (error is NpgsqlException or InvalidOperationException or OperationCanceledException or TimeoutException)
        {
            relay.StopAll();
            logger.LogCritical(error, "Relay lease was lost; stopping the backend."); lifetime.StopApplication();
        }
    }
    public async Task StopAsync(CancellationToken ct)
    {
        relay.StopAll(); await stopping.CancelAsync();
        if (monitor is not null) await monitor;
        if (connection is not null) { await connection.DisposeAsync(); connection = null; }
    }
    public async ValueTask DisposeAsync() { await StopAsync(CancellationToken.None); stopping.Dispose(); }
}
