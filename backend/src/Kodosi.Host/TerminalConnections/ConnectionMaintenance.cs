using Kodosi.Devices;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.TerminalConnections;

internal sealed class ConnectionMaintenance(IServiceScopeFactory scopes, ConnectionDirectory directory) : BackgroundService
{
    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        using var timer = new PeriodicTimer(TimeSpan.FromSeconds(5));
        while (await timer.WaitForNextTickAsync(stoppingToken))
        {
            await Parallel.ForEachAsync(directory.Online(), new ParallelOptions { MaxDegreeOfParallelism = 8, CancellationToken = stoppingToken },
                async (link, ct) =>
                {
                    try
                    {
                        await using var scope = scopes.CreateAsyncScope();
                        await scope.ServiceProvider.GetRequiredService<DeviceService>().RequireDeviceAsync(link.UserId, link.DeviceId, ct);
                        link.Send(new { type = "ping" });
                    }
                    catch (ApiException)
                    {
                        directory.RemoveDevice(link.UserId, link.DeviceId);
                    }
                    catch (Exception error) when (error is DbUpdateException or Npgsql.NpgsqlException or TimeoutException
                                                  or InvalidOperationException { InnerException: Npgsql.NpgsqlException or TimeoutException })
                    {
                    }
                });
        }
    }
}
