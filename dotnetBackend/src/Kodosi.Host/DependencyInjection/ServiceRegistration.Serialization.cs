using Kodosi.Host.Serialization;

namespace Kodosi.Host.DependencyInjection;

public static partial class ServiceRegistration
{
    private static void ConfigureJsonContracts(IServiceCollection services)
    {
        services.ConfigureHttpJsonOptions(options =>
            HostJsonSerializerOptions.Configure(options.SerializerOptions));
    }
}
