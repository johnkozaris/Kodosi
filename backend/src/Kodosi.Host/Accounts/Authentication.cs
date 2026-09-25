using Microsoft.AspNetCore.Authentication.JwtBearer;
using Microsoft.IdentityModel.Tokens;

namespace Kodosi.Accounts;

internal static class Authentication
{
    public static void AddIdentityAuthentication(this IServiceCollection services, IConfiguration config)
    {
        var authority = (config["Auth:Authority"] ?? "").Trim().TrimEnd('/');
        var audience = (config["Auth:Audience"] ?? "").Trim();
        if (!Uri.TryCreate(authority, UriKind.Absolute, out var issuer) || issuer.Query.Length > 0 || issuer.Fragment.Length > 0
            || issuer.Scheme is not ("https" or "http") || (issuer.Scheme == "http" && !issuer.IsLoopback) || audience.Length == 0)
            throw new InvalidOperationException("Configure Auth:Authority (HTTPS, or loopback HTTP for local development) and Auth:Audience.");
        services.AddAuthentication(JwtBearerDefaults.AuthenticationScheme).AddJwtBearer(options =>
        {
            options.Authority = authority;
            options.RequireHttpsMetadata = issuer.Scheme == "https";
            options.MapInboundClaims = false;
            options.TokenValidationParameters = new TokenValidationParameters
            {
                ValidateIssuer = true,
                ValidIssuer = authority,
                ValidateAudience = true,
                ValidAudience = audience,
                ValidateLifetime = true,
                ValidateIssuerSigningKey = true,
                ValidAlgorithms = ["RS256", "ES256"],
                ClockSkew = TimeSpan.FromSeconds(30),
            };
            options.Events = new JwtBearerEvents
            {
                OnTokenValidated = context =>
                {
                    if (string.IsNullOrWhiteSpace(context.Principal?.FindFirst("sub")?.Value)) context.Fail("Token identity is incomplete.");
                    return Task.CompletedTask;
                },
            };
        });
        services.AddAuthorization();
    }
}
