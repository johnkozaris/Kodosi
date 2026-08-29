using System.IdentityModel.Tokens.Jwt;

namespace Kodosi.Host.Auth;

internal static class JwtIssuerSelector
{
    public static string? ResolveScheme(
        HttpRequest request,
        IReadOnlyDictionary<string, string> schemesByIssuer,
        string? fallbackScheme)
    {
        ArgumentNullException.ThrowIfNull(request);

        var token = BearerTokenResolver.Resolve(request);
        if (string.IsNullOrWhiteSpace(token))
        {
            return fallbackScheme;
        }

        var handler = new JwtSecurityTokenHandler();
        if (!handler.CanReadToken(token))
        {
            return fallbackScheme;
        }

        try
        {
            var jwt = handler.ReadJwtToken(token);
            var issuer = jwt.Issuer?.Trim();
            if (!string.IsNullOrWhiteSpace(issuer) &&
                schemesByIssuer.TryGetValue(issuer, out var scheme))
            {
                return scheme;
            }
        }
        catch (ArgumentException)
        {
            return fallbackScheme;
        }

        return fallbackScheme;
    }
}
