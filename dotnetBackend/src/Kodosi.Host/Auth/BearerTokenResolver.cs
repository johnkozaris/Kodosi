using Kodosi.Host.Realtime;

namespace Kodosi.Host.Auth;

internal static class BearerTokenResolver
{
    public static string? Resolve(HttpRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        var authorization = request.Headers.Authorization.ToString();
        if (!string.IsNullOrWhiteSpace(authorization) &&
            authorization.StartsWith("Bearer ", StringComparison.OrdinalIgnoreCase))
        {
            var token = authorization["Bearer ".Length..].Trim();
            if (!string.IsNullOrWhiteSpace(token))
            {
                return token;
            }
        }

        return WebSocketAccessTokenResolver.Resolve(request);
    }
}
