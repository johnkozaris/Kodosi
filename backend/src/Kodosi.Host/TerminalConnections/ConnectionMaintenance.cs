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
            directory.Sweep();
            var groups = directory.OnlinePeers().GroupBy(peer => (peer.UserId, peer.DeviceId));
            await Parallel.ForEachAsync(groups, new ParallelOptions { MaxDegreeOfParallelism = 8, CancellationToken = stoppingToken },
                async (peers, ct) =>
                {
                    try
                    {
                        await using var scope = scopes.CreateAsyncScope();
                        await scope.ServiceProvider.GetRequiredService<DeviceService>().RequireDeviceAsync(peers.Key.UserId, peers.Key.DeviceId, ct);
                        foreach (var peer in peers) peer.Send(new { type = "ping" });
                    }
                    catch (Exception error) when (error is ApiException or DbUpdateException or Npgsql.NpgsqlException)
                    {
                        foreach (var peer in peers) peer.Abort();
                    }
                });
        }
    }
}
