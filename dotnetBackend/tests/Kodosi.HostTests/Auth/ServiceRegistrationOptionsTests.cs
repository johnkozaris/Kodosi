using System.Security.Claims;
using Kodosi.Host.Auth;
using Kodosi.Host.Configuration;
using Kodosi.Host.DependencyInjection;
using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Authentication.JwtBearer;
using Microsoft.AspNetCore.Http;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Options;

namespace Kodosi.HostTests;

public sealed class ServiceRegistrationOptionsTests
{
    [Fact]
    public async Task TokenValidation_Produces_Complete_Profile_Feature_Without_Synthetic_Profile_Claims()
    {
        var services = new ServiceCollection();
        services.AddLogging();
        services.AddHostAdapters(BuildAppleLinkedAuthConfiguration());
        using var provider = services.BuildServiceProvider();
        var jwtOptions = provider.GetRequiredService<IOptionsMonitor<JwtBearerOptions>>()
            .Get("authentik");
        var httpContext = new DefaultHttpContext
        {
            RequestServices = provider,
            User = new ClaimsPrincipal(new ClaimsIdentity(
                [
                    new Claim("sub", "authentik-sub"),
                    new Claim("iss", "https://auth.kodosi.test/application/o/kodosi/"),
                    new Claim("preferred_username", "john"),
                    new Claim("email", "john@example.test"),
                    new Claim("name", "John Example"),
                    new Claim("picture", "https://cdn.example.test/avatar.png"),
                    new Claim("kodosi_apple_subject", "apple-sub"),
                ],
                authenticationType: "authentik")),
        };
        var context = new TokenValidatedContext(
            httpContext,
            new AuthenticationScheme("authentik", null, typeof(JwtBearerHandler)),
            jwtOptions)
        {
            Principal = httpContext.User,
        };

        await jwtOptions.Events.OnTokenValidated(context);

        Assert.Null(context.Result);
        var profile = Assert.IsType<AuthenticatedProfileFeature>(
            httpContext.Features.Get<AuthenticatedProfileFeature>()).Profile;
        Assert.Equal("apple", profile.PrimaryIdentity.Provider);
        Assert.Equal("apple-sub", profile.PrimaryIdentity.Subject);
        Assert.Equal(2, profile.Identities.Count);
        Assert.Equal("john", profile.HandleSeed);
        Assert.Equal("john@example.test", profile.Email);
        Assert.Equal("John Example", profile.DisplayName);
        Assert.Equal("https://cdn.example.test/avatar.png", profile.AvatarUrl);
        Assert.DoesNotContain(
            httpContext.User.Claims,
            claim => claim.Type is
                "kodosi_identities" or
                "kodosi_handle_seed" or
                "kodosi_email" or
                "kodosi_display_name" or
                "kodosi_bootstrap_display_name" or
                "kodosi_avatar_url");
    }

    [Fact]
    public void AddHostAdapters_Registers_CurrentUser_As_Host_Scoped_Service()
    {
        var services = new ServiceCollection();

        services.AddHostAdapters(BuildAuthConfiguration(
            ("test", "https://issuer.test", null)));

        var registration = Assert.Single(services, descriptor =>
            descriptor.ServiceType == typeof(ICurrentUser));
        Assert.Equal(ServiceLifetime.Scoped, registration.Lifetime);
        Assert.Equal(typeof(AuthenticatedCurrentUser), registration.ImplementationType);
    }

    [Fact]
    public void AddHostAdapters_Binds_AuthenticatedUserSyncTtl()
    {
        var configuration = new ConfigurationBuilder()
            .AddInMemoryCollection(new Dictionary<string, string?>
            {
                ["Auth:LocalProfileSyncTtlSeconds"] = "7",
                ["Auth:Providers:0:Enabled"] = "true",
                ["Auth:Providers:0:Scheme"] = "test",
                ["Auth:Providers:0:Provider"] = "test",
                ["Auth:Providers:0:Authority"] = "https://issuer.test",
                ["Auth:Providers:0:CanonicalIssuer"] = "https://issuer.test",
                ["Auth:Providers:0:Audiences:0"] = "test-audience",
                ["Auth:Providers:0:AllowedAlgorithms:0"] = "RS256",
                ["Telemetry:ServiceName"] = "Kodosi.Tests",
            })
            .Build();
        var services = new ServiceCollection();

        services.AddHostAdapters(configuration);
        using var provider = services.BuildServiceProvider();

        Assert.Equal(
            7,
            provider.GetRequiredService<IOptions<OidcAuthOptions>>()
                .Value.LocalProfileSyncTtlSeconds);
    }

    [Fact]
    public void AddHostAdapters_Rejects_Duplicate_Enabled_Scheme_Names()
    {
        var configuration = BuildAuthConfiguration(
            ("shared", "https://issuer-a.test", null),
            ("shared", "https://issuer-b.test", null));

        var error = Assert.Throws<InvalidOperationException>(() =>
            new ServiceCollection().AddHostAdapters(configuration));

        Assert.Contains("scheme 'shared' is configured more than once", error.Message);
    }

    [Fact]
    public void AddHostAdapters_Rejects_Reserved_Bearer_Scheme()
    {
        var configuration = BuildAuthConfiguration(
            ("Bearer", "https://issuer.test", null));

        var error = Assert.Throws<InvalidOperationException>(() =>
            new ServiceCollection().AddHostAdapters(configuration));

        Assert.Contains(
            "scheme 'Bearer' is reserved for the bearer policy",
            error.Message);
    }

    [Fact]
    public void AddHostAdapters_Rejects_Normalized_Issuer_Owned_By_Different_Schemes()
    {
        var configuration = BuildAuthConfiguration(
            ("scheme-a", "https://issuer.test/", "https://issuer.test/"),
            ("scheme-b", "https://other.test", " https://issuer.test "));

        var error = Assert.Throws<InvalidOperationException>(() =>
            new ServiceCollection().AddHostAdapters(configuration));

        Assert.Contains(
            "Auth issuer 'https://issuer.test' is assigned to both 'scheme-a' and 'scheme-b'",
            error.Message);
    }

    [Fact]
    public void AddHostAdapters_Permits_Duplicate_Issuer_Aliases_Within_One_Provider()
    {
        var values = ValidProviderValues(0, "scheme-a", "https://issuer.test/");
        values["Auth:Providers:0:CanonicalIssuer"] = "https://issuer.test";
        values["Auth:Providers:0:ValidIssuers:0"] = " https://issuer.test/ ";
        values["Auth:Providers:0:ValidIssuers:1"] = "https://issuer.test";
        var configuration = new ConfigurationBuilder()
            .AddInMemoryCollection(values)
            .Build();
        var services = new ServiceCollection();

        services.AddHostAdapters(configuration);
        using var provider = services.BuildServiceProvider();

        Assert.Equal(
            "scheme-a",
            provider.GetRequiredService<IOptions<OidcAuthOptions>>()
                .Value.Providers.Single().Scheme);
    }

    [Fact]
    public void AddHostAdapters_Rejects_Invalid_Fixed_Window_Options()
    {
        foreach (var policy in new[]
        {
            "Api",
            "WebSockets",
            "DestructiveIdentity",
            "DestructiveEnrollment",
            "DeviceProof",
            "DeviceLinkInit",
            "DeviceLinkPoll",
        })
        {
            foreach (var (property, value) in new[]
            {
                ("PermitLimit", "0"),
                ("WindowSeconds", "0"),
                ("QueueLimit", "-1"),
            })
            {
                var key = $"RateLimiting:{policy}:{property}";
                var error = Assert.Throws<InvalidOperationException>(() =>
                    new ServiceCollection().AddHostAdapters(
                        BuildConfigurationWithOverride(key, value)));

                Assert.Contains(key, error.Message);
            }
        }
    }

    [Theory]
    [InlineData("WebSockets:KeepAliveIntervalSeconds")]
    [InlineData("RequestLimits:MaxRequestBodySizeBytes")]
    public void AddHostAdapters_Rejects_Nonpositive_Host_Options(string key)
    {
        var error = Assert.Throws<InvalidOperationException>(() =>
            new ServiceCollection().AddHostAdapters(
                BuildConfigurationWithOverride(key, "0")));

        Assert.Contains(key, error.Message);
    }

    [Fact]
    public void AddHostAdapters_Accepts_Numeric_Host_Option_Boundaries()
    {
        var values = ValidProviderValues(0, "test", "https://issuer.test");
        foreach (var policy in new[]
        {
            "Api",
            "WebSockets",
            "DestructiveIdentity",
            "DestructiveEnrollment",
            "DeviceProof",
            "DeviceLinkInit",
            "DeviceLinkPoll",
        })
        {
            values[$"RateLimiting:{policy}:PermitLimit"] = "1";
            values[$"RateLimiting:{policy}:WindowSeconds"] = "1";
            values[$"RateLimiting:{policy}:QueueLimit"] = "0";
        }
        values["WebSockets:KeepAliveIntervalSeconds"] = "1";
        values["RequestLimits:MaxRequestBodySizeBytes"] = "1";
        var configuration = new ConfigurationBuilder()
            .AddInMemoryCollection(values)
            .Build();

        new ServiceCollection().AddHostAdapters(configuration);
    }

    private static IConfiguration BuildConfigurationWithOverride(string key, string value)
    {
        var values = ValidProviderValues(0, "test", "https://issuer.test");
        values[key] = value;
        return new ConfigurationBuilder()
            .AddInMemoryCollection(values)
            .Build();
    }

    private static IConfiguration BuildAppleLinkedAuthConfiguration()
    {
        var values = ValidProviderValues(
            0,
            "authentik",
            "https://auth.kodosi.test/application/o/kodosi/");
        values["Auth:Providers:0:Provider"] = "authentik";
        values["Auth:Providers:0:CanonicalIssuer"] =
            "https://auth.kodosi.test/application/o/kodosi/";
        values["Auth:Providers:0:LinkedIdentityClaims:0:Provider"] = "apple";
        values["Auth:Providers:0:LinkedIdentityClaims:0:Issuer"] =
            "https://appleid.apple.com";
        values["Auth:Providers:0:LinkedIdentityClaims:0:SubjectClaim"] =
            "kodosi_apple_subject";
        return new ConfigurationBuilder()
            .AddInMemoryCollection(values)
            .Build();
    }

    private static IConfiguration BuildAuthConfiguration(
        params (string Scheme, string Authority, string? Issuer)[] providers)
    {
        var values = new Dictionary<string, string?>();
        for (var index = 0; index < providers.Length; index++)
        {
            var provider = providers[index];
            foreach (var pair in ValidProviderValues(
                index,
                provider.Scheme,
                provider.Authority))
            {
                values[pair.Key] = pair.Value;
            }

            if (provider.Issuer is not null)
            {
                values[$"Auth:Providers:{index}:CanonicalIssuer"] = provider.Issuer;
            }
        }

        return new ConfigurationBuilder()
            .AddInMemoryCollection(values)
            .Build();
    }

    private static Dictionary<string, string?> ValidProviderValues(
        int index,
        string scheme,
        string authority) =>
        new()
        {
            [$"Auth:Providers:{index}:Enabled"] = "true",
            [$"Auth:Providers:{index}:Scheme"] = scheme,
            [$"Auth:Providers:{index}:Provider"] = $"provider-{index}",
            [$"Auth:Providers:{index}:Authority"] = authority,
            [$"Auth:Providers:{index}:Audiences:0"] = $"audience-{index}",
            [$"Auth:Providers:{index}:AllowedAlgorithms:0"] = "RS256",
            ["Telemetry:ServiceName"] = "Kodosi.Tests",
        };
}
