using Kodosi.Application;
using Kodosi.Infrastructure.Persistence.Repositories;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.Host.DependencyInjection;

public static class RealtimeInfrastructureModule
{
    public static IServiceCollection AddRealtimeInfrastructure(this IServiceCollection services)
    {
        services.AddSingleton<IConnectionRegistry, ConnectionRegistry>();
        services.AddSingleton<ILiveSessionStateDirectory, LiveSessionStateDirectory>();
        services.AddSingleton<IActionDedupeCache, ActionDedupeCache>();
        services.AddSingleton<ISemanticRelayRepository, SemanticRelayRepository>();
        services.AddSingleton<IPermissionDecisionAuditStore, PermissionDecisionAuditStore>();

        services.AddSingleton<AuditWriter>();
        services.AddSingleton<IAuditWriter>(sp => sp.GetRequiredService<AuditWriter>());
        services.AddHostedService(sp => sp.GetRequiredService<AuditWriter>());

        return services;
    }
}
