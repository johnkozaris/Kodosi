using Microsoft.AspNetCore.Authentication.JwtBearer;
using Microsoft.IdentityModel.JsonWebTokens;
using Microsoft.IdentityModel.Tokens;

namespace Kodosi.Accounts;

internal static class Authentication
{
    internal const string CanonicalIssuerItem = "Kodosi.CanonicalIssuer";

    public static void AddIdentityAuthentication(this IServiceCollection services, IConfiguration config)
    {
        var providers = config.GetSection("Auth:Providers").Get<AuthProvider[]>()?.Where(x => x.Enabled).ToArray() ?? [];
        if (providers.Length == 0) throw new InvalidOperationException("Configure at least one Auth:Providers entry.");
        var schemes = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var provider in providers)
        {
            if (string.IsNullOrWhiteSpace(provider.Scheme) || provider.Scheme == "Bearer"
                || !Uri.TryCreate(provider.Authority, UriKind.Absolute, out var authority)
                || authority.Scheme is not ("https" or "http") || provider.Audiences.Length == 0
                || provider.AllowedAlgorithms.Length == 0 || provider.AllowedAlgorithms.Any(x => x is not ("RS256" or "ES256" or "ES384" or "RS384" or "RS512")))
                throw new InvalidOperationException("Invalid OIDC provider configuration.");
            foreach (var issuer in provider.Issuers())
                if (!schemes.TryAdd(issuer, provider.Scheme)) throw new InvalidOperationException("An OIDC issuer is configured more than once.");
        }
        var auth = services.AddAuthentication("Bearer").AddPolicyScheme("Bearer", "External bearer", options =>
        {
            options.ForwardDefaultSelector = context =>
            {
                var header = context.Request.Headers.Authorization.ToString();
                if (header.StartsWith("Bearer ", StringComparison.OrdinalIgnoreCase) && header.Length < 32768)
                {
                    try
                    {
                        var token = new JsonWebToken(header[7..].Trim());
                        if (schemes.TryGetValue(token.Issuer, out var scheme)) return scheme;
                    }
                    catch (ArgumentException) { }
                }
                return providers[0].Scheme;
            };
        });
        foreach (var provider in providers)
            auth.AddJwtBearer(provider.Scheme, options =>
            {
                options.Authority = provider.Authority.TrimEnd('/'); options.RequireHttpsMetadata = provider.RequireHttpsMetadata;
                options.MapInboundClaims = false;
                options.TokenValidationParameters = new TokenValidationParameters
                {
                    ValidateIssuer = true,
                    ValidIssuers = provider.Issuers(),
                    ValidateAudience = true,
                    ValidAudiences = provider.Audiences,
                    ValidateLifetime = true,
                    ValidateIssuerSigningKey = true,
                    ValidAlgorithms = provider.AllowedAlgorithms,
                    ClockSkew = TimeSpan.FromSeconds(30),
                };
                options.Events = new JwtBearerEvents
                {
                    OnTokenValidated = context =>
                    {
                        if (string.IsNullOrWhiteSpace(context.Principal?.FindFirst("sub")?.Value)
                            || string.IsNullOrWhiteSpace(context.Principal.FindFirst("iss")?.Value)) context.Fail("Token identity is incomplete.");
                        else context.HttpContext.Items[CanonicalIssuerItem] = string.IsNullOrWhiteSpace(provider.CanonicalIssuer)
                            ? provider.Authority.Trim() : provider.CanonicalIssuer.Trim();
                        return Task.CompletedTask;
                    },
                };
            });
        services.AddAuthorization();
    }
    private sealed class AuthProvider
    {
        public bool Enabled { get; set; } = true;
        public string Scheme { get; set; } = "";
        public string Authority { get; set; } = "";
        public string CanonicalIssuer { get; set; } = "";
        public string[] ValidIssuers { get; set; } = [];
        public string[] Audiences { get; set; } = [];
        public string[] AllowedAlgorithms { get; set; } = ["RS256", "ES256"];
        public bool RequireHttpsMetadata { get; set; } = true;
        public string[] Issuers() => ValidIssuers.Append(string.IsNullOrWhiteSpace(CanonicalIssuer) ? Authority : CanonicalIssuer).Distinct(StringComparer.Ordinal).ToArray();
    }
}
