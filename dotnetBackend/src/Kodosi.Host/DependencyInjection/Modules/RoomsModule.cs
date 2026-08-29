using Kodosi.Application;

namespace Kodosi.Host.DependencyInjection;

public static class RoomsModule
{
    public static IServiceCollection AddRoomsModule(this IServiceCollection services)
    {
        services.AddScoped<RoomRosterVerifier>();
        services.AddScoped<RoomInvitationProofVerifier>();
        services.AddScoped<RoomService>();
        services.AddScoped<RoomCatalogQueryService>();
        services.AddScoped<RoomMembershipWorkflowService>();
        services.AddScoped<RoomInvitationService>();
        services.AddScoped<RoomChatService>();
        services.AddScoped<RoomTaskService>();
        services.AddScoped<RoomMutationReceiptLookup>();
        services.AddScoped<RoomSessionFeedService>();
        return services;
    }
}
