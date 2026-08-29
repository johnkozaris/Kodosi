using Kodosi.Application;

namespace Kodosi.Host;

internal sealed class RealtimeCounterResetHostedService(
    IServiceScopeFactory scopeFactory,
    ILogger<RealtimeCounterResetHostedService> logger) : IHostedService
{
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly ILogger<RealtimeCounterResetHostedService> _logger = logger;

    public async Task StartAsync(CancellationToken cancellationToken)
    {


        using var scope = _scopeFactory.CreateScope();
        var repository = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
        await repository.ResetRealtimeCountersAsync(cancellationToken);
        _logger.LogInformation("Realtime counters reset at startup");
    }

    public Task StopAsync(CancellationToken cancellationToken) => Task.CompletedTask;
}
