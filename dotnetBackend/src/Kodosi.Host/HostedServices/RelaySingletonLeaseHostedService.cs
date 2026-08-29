using Npgsql;

namespace Kodosi.Host;

internal sealed class RelaySingletonLeaseHostedService(
    NpgsqlDataSource dataSource,
    IHostApplicationLifetime applicationLifetime,
    ILogger<RelaySingletonLeaseHostedService> logger) : IHostedService, IDisposable
{
    private const long AdvisoryLockKey = 0x4B4F444F5349;
    private static readonly TimeSpan LeaseCheckInterval = TimeSpan.FromSeconds(5);

    private readonly NpgsqlDataSource _dataSource = dataSource;
    private readonly IHostApplicationLifetime _applicationLifetime = applicationLifetime;
    private readonly ILogger<RelaySingletonLeaseHostedService> _logger = logger;
    private readonly SemaphoreSlim _startGate = new(1, 1);
    private NpgsqlConnection? _leaseConnection;
    private CancellationTokenSource? _monitorCancellation;
    private Task? _monitorTask;

    public async Task StartAsync(CancellationToken cancellationToken)
    {
        await _startGate.WaitAsync(cancellationToken);
        try
        {
            if (_leaseConnection is not null)
            {
                return;
            }

            var connection = await _dataSource.OpenConnectionAsync(cancellationToken);
            try
            {
                await using var command = new NpgsqlCommand(
                    "SELECT pg_try_advisory_lock(@key)",
                    connection);
                command.Parameters.AddWithValue("key", AdvisoryLockKey);
                var acquired = await command.ExecuteScalarAsync(cancellationToken) as bool?;
                if (acquired != true)
                {
                    throw new InvalidOperationException(
                        "Another Kodosi relay instance already holds the PostgreSQL singleton lease.");
                }
            }
            catch
            {
                await connection.DisposeAsync();
                throw;
            }

            _leaseConnection = connection;
            _monitorCancellation = new CancellationTokenSource();
            _monitorTask = MonitorLeaseAsync(connection, _monitorCancellation.Token);
            _logger.LogInformation("PostgreSQL relay singleton lease acquired");
        }
        finally
        {
            _startGate.Release();
        }
    }

    public async Task StopAsync(CancellationToken cancellationToken)
    {
        await _startGate.WaitAsync(CancellationToken.None);
        try
        {
            var monitorCancellation = Interlocked.Exchange(ref _monitorCancellation, null);
            var monitorTask = Interlocked.Exchange(ref _monitorTask, null);
            if (monitorCancellation is not null)
            {
                await monitorCancellation.CancelAsync();
            }
            if (monitorTask is not null)
            {
                await monitorTask;
            }
            monitorCancellation?.Dispose();

            var connection = Interlocked.Exchange(ref _leaseConnection, null);
            if (connection is null)
            {
                return;
            }

            try
            {
                if (connection.State == System.Data.ConnectionState.Open)
                {
                    await using var command = new NpgsqlCommand(
                        "SELECT pg_advisory_unlock(@key)",
                        connection);
                    command.Parameters.AddWithValue("key", AdvisoryLockKey);
                    await command.ExecuteNonQueryAsync(CancellationToken.None);
                }
            }
            catch (NpgsqlException exception)
            {
                _logger.LogWarning(
                    exception,
                    "PostgreSQL relay singleton lease connection was already lost");
            }
            finally
            {
                await connection.DisposeAsync();
            }
        }
        finally
        {
            _startGate.Release();
        }
    }

    public void Dispose() => _startGate.Dispose();

    private async Task MonitorLeaseAsync(
        NpgsqlConnection connection,
        CancellationToken cancellationToken)
    {
        using var timer = new PeriodicTimer(LeaseCheckInterval);
        try
        {
            while (await timer.WaitForNextTickAsync(cancellationToken))
            {
                await using var command = new NpgsqlCommand("SELECT 1", connection);
                await command.ExecuteScalarAsync(cancellationToken);
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception) when (exception is NpgsqlException or InvalidOperationException)
        {
            _logger.LogCritical(
                exception,
                "PostgreSQL relay singleton lease was lost; stopping to preserve single-relay authority");
            _applicationLifetime.StopApplication();
        }
    }
}
