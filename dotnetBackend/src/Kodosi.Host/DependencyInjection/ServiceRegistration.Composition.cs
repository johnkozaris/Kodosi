namespace Kodosi.Host.DependencyInjection;

public static partial class ServiceRegistration
{
    public static IServiceCollection AddApplication(this IServiceCollection services)
    {
        return services
            .AddSessionsModule()
            .AddSharingModule()
            .AddIdentityModule()
            .AddRoomsModule()
            .AddSocialModule()
            .AddRealtimeApplicationModule();
    }

    public static IServiceCollection AddInfrastructure(
        this IServiceCollection services,
        IConfiguration configuration)
    {
        return services
            .AddPersistenceInfrastructure(configuration)
            .AddRealtimeInfrastructure()
            .AddCryptoInfrastructure()
            .AddRateLimitingInfrastructure(configuration);
    }
}
