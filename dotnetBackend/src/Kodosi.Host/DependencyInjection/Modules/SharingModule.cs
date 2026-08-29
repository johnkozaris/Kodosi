using Kodosi.Application;

namespace Kodosi.Host.DependencyInjection;

public static class SharingModule
{
    public static IServiceCollection AddSharingModule(this IServiceCollection services)
    {
        services.AddScoped<SessionAccessService>();
        services.AddScoped<SessionViewerDismissalService>();
        services.AddScoped<SharingService>();
        services.AddScoped<SessionAccessQueryService>();
        services.AddScoped<SessionKeyDistributionService>();
        services.AddScoped<SessionKeyQueryService>();
        services.AddScoped<AuthorizedSessionDeviceQueryService>();
        services.AddScoped<ISemanticReceiptVerifier, SemanticReceiptVerifier>();
        services.AddScoped<SemanticReceiptAckVerifier>();
        services.AddScoped<SemanticReceiptMailboxService>();
        services.AddScoped<SessionAccessMutationReceiptLookup>();
        return services;
    }
}
