using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.Json.Serialization.Metadata;
using Kodosi.Application;

namespace Kodosi.Host.Serialization;

public static class HostJsonSerializerOptions
{
    public static void Configure(JsonSerializerOptions options)
    {
        ArgumentNullException.ThrowIfNull(options);

        if (!options.Converters.Any(converter => converter is JsonStringEnumConverter))
        {
            options.Converters.Add(new JsonStringEnumConverter(namingPolicy: null, allowIntegerValues: false));
        }



        var chain = options.TypeInfoResolverChain;
        if (!chain.Any(resolver => resolver is AppJsonContext))
        {
            chain.Insert(0, AppJsonContext.Default);
        }
        if (!chain.Any(resolver => resolver is DefaultJsonTypeInfoResolver))
        {
            chain.Add(new DefaultJsonTypeInfoResolver());
        }
    }
}
