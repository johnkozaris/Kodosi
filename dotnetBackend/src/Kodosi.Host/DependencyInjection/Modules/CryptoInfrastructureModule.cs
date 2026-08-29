using Kodosi.Application;
using Kodosi.Infrastructure.Crypto;
using Kodosi.Domain;

namespace Kodosi.Host.DependencyInjection;

public static class CryptoInfrastructureModule
{
    public static IServiceCollection AddCryptoInfrastructure(this IServiceCollection services)
    {
        services.AddSingleton<IOwnerSessionSecretHasher, OwnerSessionSecretHasher>();
        services.AddSingleton<IPopSignatureVerifier, MLDsaPopSignatureVerifier>();
        services.AddSingleton<DeviceCertificateParser>();
        services.AddSingleton<SignedDeviceListParser>();
        return services;
    }
}
