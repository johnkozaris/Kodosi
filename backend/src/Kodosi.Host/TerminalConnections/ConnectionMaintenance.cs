using Kodosi.Devices;

namespace Kodosi.TerminalConnections;

/// <summary>Each minute, checks that each connected device is still approved, in two queries for each thousand devices.</summary>
internal sealed class ConnectionMaintenance(IServiceScopeFactory scopes, ConnectionDirectory directory, TimeProvider clock,
    ILogger<ConnectionMaintenance> logger) : PeriodicWork(TimeSpan.FromSeconds(60), clock, logger)
{
    private const int Batch = 1000;

    protected internal override async Task RunAsync(CancellationToken ct)
    {
        foreach (var batch in directory.Online().Chunk(Batch))
        {
            await using var scope = scopes.CreateAsyncScope();
            var approved = await scope.ServiceProvider.GetRequiredService<DeviceService>()
                .ApprovedAsync(batch.Select(x => (x.UserId, x.DeviceId)).ToArray(), ct);
            foreach (var connection in batch.Where(x => !approved.Contains((x.UserId, x.DeviceId))))
                directory.RemoveDevice(connection.UserId, connection.DeviceId);
        }
    }
}

/// <summary>
/// Ends a device connection that sent nothing for <see cref="Silence"/>, or whose current send waits for longer than
/// <see cref="SendLimit"/>. One sweep replaces a timer for each message.
/// </summary>
internal sealed class ConnectionWatchdog(ConnectionDirectory directory, TimeProvider clock, ILogger<ConnectionWatchdog> logger)
    : PeriodicWork(TimeSpan.FromSeconds(5), clock, logger)
{
    internal static readonly TimeSpan Silence = TimeSpan.FromSeconds(90);
    internal static readonly TimeSpan SendLimit = TimeSpan.FromSeconds(30);

    protected internal override Task RunAsync(CancellationToken ct)
    {
        foreach (var connection in directory.Online())
            if (connection.Silence > Silence || connection.Sending > SendLimit) connection.Abort();
        return Task.CompletedTask;
    }
}
