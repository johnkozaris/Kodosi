using System.Security.Claims;
using Microsoft.AspNetCore.Authentication.JwtBearer;
using Microsoft.Extensions.Options;
using Microsoft.IdentityModel.Tokens;
using Kodosi.Host.Auth;
using Kodosi.Host.Configuration;

namespace Kodosi.Host.DependencyInjection;

public static partial class ServiceRegistration
{
    private static void ConfigureAuthentication(
        IServiceCollection services,
        OidcAuthOptions authOptions)
    {
        var enabledProviders = authOptions.Providers
            .Where(provider => provider.Enabled)
            .ToArray();

        if (enabledProviders.Length == 0)
        {
            throw new InvalidOperationException("Auth:Providers must contain at least one enabled provider.");
        }

        var schemesByIssuer = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var provider in enabledProviders)
        {
            foreach (var issuer in ResolveValidIssuers(provider))
            {
                schemesByIssuer[issuer] = provider.Scheme;
            }
        }

        var fallbackScheme = enabledProviders[0].Scheme;
        var builder = services.AddAuthentication(options =>
            {
                options.DefaultAuthenticateScheme = JwtBearerDefaults.AuthenticationScheme;
                options.DefaultChallengeScheme = JwtBearerDefaults.AuthenticationScheme;
            })
            .AddPolicyScheme(JwtBearerDefaults.AuthenticationScheme, "External bearer", options =>
            {
                options.ForwardDefaultSelector = context =>
                    JwtIssuerSelector.ResolveScheme(context.Request, schemesByIssuer, fallbackScheme);
            });



        services.AddSingleton<IJwtRevalidator>(sp => new JwtRevalidator(
            sp.GetRequiredService<IOptionsMonitor<JwtBearerOptions>>(),
            schemesByIssuer,
            fallbackScheme,
            sp.GetRequiredService<ILogger<JwtRevalidator>>(),
            sp.GetRequiredService<Observability.OperationalMetrics>()));

        foreach (var provider in enabledProviders)
        {
            builder.AddJwtBearer(provider.Scheme, options =>
            {
                options.Authority = TrimTrailingSlash(provider.Authority);
                options.RequireHttpsMetadata = provider.RequireHttpsMetadata;
                options.MapInboundClaims = false;
                options.TokenValidationParameters = new TokenValidationParameters
                {
                    ValidateIssuer = true,
                    ValidIssuers = ResolveValidIssuers(provider),
                    ValidateAudience = provider.Audiences.Length > 0,
                    ValidAudiences = provider.Audiences,
                    ValidateLifetime = true,
                    ValidateIssuerSigningKey = true,
                    ValidAlgorithms = provider.AllowedAlgorithms,
                    NameClaimType = ClaimTypes.Name,



                    ClockSkew = TimeSpan.FromMinutes(2),
                };
                options.Events = new JwtBearerEvents
                {
                    OnMessageReceived = context =>
                    {
                        context.Token = BearerTokenResolver.Resolve(context.HttpContext.Request);
                        return Task.CompletedTask;
                    },
                    OnTokenValidated = context =>
                    {
                        if (context.Principal is null)
                        {
                            LogAuthenticationFailure(
                                context.HttpContext,
                                provider.Scheme,
                                "Authenticated token is missing the principal.");
                            context.Fail("Authenticated token is missing the principal.");
                            return Task.CompletedTask;
                        }

                        if (!AuthenticatedUserProfileParser.TryParse(
                            context.Principal,
                            provider,
                            out var profile,
                            out var error))
                        {
                            LogAuthenticationFailure(
                                context.HttpContext,
                                provider.Scheme,
                                error ?? "Unable to resolve the authenticated user profile.");
                            context.Fail(error ?? "Unable to resolve the authenticated user profile.");
                            return Task.CompletedTask;
                        }

                        if (context.Principal.Identity is not ClaimsIdentity identity)
                        {
                            LogAuthenticationFailure(
                                context.HttpContext,
                                provider.Scheme,
                                "Authenticated token identity is not writable.");
                            context.Fail("Authenticated token identity is not writable.");
                            return Task.CompletedTask;
                        }



                        context.HttpContext.Features.Set(new AuthenticatedProfileFeature(profile));
                        return Task.CompletedTask;
                    },
                    OnAuthenticationFailed = context =>
                    {
                        var logger = context.HttpContext.RequestServices
                            .GetRequiredService<ILoggerFactory>()
                            .CreateLogger("Kodosi.Host.Auth.Bearer");
                        logger.LogDebug(
                            "JWT validation failed for scheme {Scheme}: {FailureType}",
                            provider.Scheme,
                            context.Exception.GetType().Name);
                        return Task.CompletedTask;
                    },
                };
            });
        }

        services.AddAuthorization();
    }

    private static void LogAuthenticationFailure(
        HttpContext context,
        string scheme,
        string reason)
    {
        var logger = context.RequestServices
            .GetRequiredService<ILoggerFactory>()
            .CreateLogger("Kodosi.Host.Auth.Bearer");
        logger.LogWarning(
            "JWT profile normalization failed for scheme {Scheme}: {Reason}",
            scheme,
            reason);
    }

    private static void ValidateAuthOptions(OidcAuthOptions authOptions)
    {
        if (authOptions.LocalProfileSyncTtlSeconds < 1)
        {
            throw new InvalidOperationException("Auth:LocalProfileSyncTtlSeconds must be at least 1.");
        }

        var enabledProviders = authOptions.Providers
            .Where(provider => provider.Enabled)
            .ToArray();
        var schemeNames = new HashSet<string>(StringComparer.Ordinal);
        var schemeByNormalizedIssuer = new Dictionary<string, string>(StringComparer.Ordinal);

        foreach (var provider in enabledProviders)
        {
            if (string.IsNullOrWhiteSpace(provider.Scheme))
            {
                throw new InvalidOperationException("Auth:Providers:Scheme must be configured.");
            }

            if (string.Equals(
                    provider.Scheme,
                    JwtBearerDefaults.AuthenticationScheme,
                    StringComparison.Ordinal))
            {
                throw new InvalidOperationException(
                    $"Auth provider scheme '{provider.Scheme}' is reserved for the bearer policy.");
            }

            if (!schemeNames.Add(provider.Scheme))
            {
                throw new InvalidOperationException(
                    $"Enabled auth provider scheme '{provider.Scheme}' is configured more than once.");
            }

            if (string.IsNullOrWhiteSpace(provider.Provider))
            {
                throw new InvalidOperationException("Auth:Providers:Provider must be configured.");
            }

            if (string.IsNullOrWhiteSpace(provider.Authority))
            {
                throw new InvalidOperationException($"Auth provider '{provider.Scheme}' must configure Authority.");
            }

            if (provider.Audiences.Length == 0 || provider.Audiences.All(string.IsNullOrWhiteSpace))
            {
                throw new InvalidOperationException($"Auth provider '{provider.Scheme}' must configure at least one audience.");
            }

            if (provider.AllowedAlgorithms.Length == 0 || provider.AllowedAlgorithms.All(string.IsNullOrWhiteSpace))
            {
                throw new InvalidOperationException($"Auth provider '{provider.Scheme}' must configure at least one signing algorithm.");
            }

            if (string.IsNullOrWhiteSpace(provider.SubjectClaim))
            {
                throw new InvalidOperationException($"Auth provider '{provider.Scheme}' must configure SubjectClaim.");
            }

            foreach (var linkedIdentity in provider.LinkedIdentityClaims)
            {
                if (string.IsNullOrWhiteSpace(linkedIdentity.Provider)
                    || string.IsNullOrWhiteSpace(linkedIdentity.Issuer)
                    || string.IsNullOrWhiteSpace(linkedIdentity.SubjectClaim))
                {
                    throw new InvalidOperationException(
                        $"Auth provider '{provider.Scheme}' contains an incomplete linked identity claim mapping.");
                }
            }

            foreach (var issuer in ResolveValidIssuers(provider)
                .Select(NormalizeIssuer)
                .Distinct(StringComparer.Ordinal))
            {
                if (schemeByNormalizedIssuer.TryGetValue(issuer, out var ownerScheme)
                    && !string.Equals(ownerScheme, provider.Scheme, StringComparison.Ordinal))
                {
                    throw new InvalidOperationException(
                        $"Auth issuer '{issuer}' is assigned to both '{ownerScheme}' and '{provider.Scheme}'.");
                }

                schemeByNormalizedIssuer[issuer] = provider.Scheme;
            }
        }
    }

    private static string TrimTrailingSlash(string authority)
        => authority.Trim().TrimEnd('/');

    private static string NormalizeIssuer(string issuer) =>
        TrimTrailingSlash(issuer);

    private static string[] ResolveValidIssuers(ExternalAuthProviderOptions provider)
    {
        var issuers = provider.ValidIssuers
            .Where(static issuer => !string.IsNullOrWhiteSpace(issuer))
            .Select(issuer => issuer.Trim())
            .ToList();

        if (!string.IsNullOrWhiteSpace(provider.CanonicalIssuer))
        {
            issuers.Add(provider.CanonicalIssuer.Trim());
        }

        if (issuers.Count == 0)
        {
            issuers.Add(TrimTrailingSlash(provider.Authority));
        }

        return issuers.Distinct(StringComparer.Ordinal).ToArray();
    }
}
