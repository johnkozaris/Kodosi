using Kodosi.Application;

namespace Kodosi.Host.DependencyInjection;

public static class SocialModule
{
    public static IServiceCollection AddSocialModule(this IServiceCollection services)
    {
        services.AddScoped<UserService>();
        services.AddScoped<FriendshipWorkflowService>();
        return services;
    }
}
