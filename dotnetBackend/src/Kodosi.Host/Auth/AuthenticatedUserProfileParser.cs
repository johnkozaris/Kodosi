using System.Security.Claims;
using Kodosi.Application;
using Kodosi.Host.Configuration;

namespace Kodosi.Host.Auth;

public static partial class AuthenticatedUserProfileParser
{
    public static bool TryParse(
        ClaimsPrincipal principal,
        ExternalAuthProviderOptions options,
        out AuthenticatedUserProfile profile,
        out string? error)
    {
        profile = default!;
        if (!TryResolveBrokerIdentitySet(principal, options, out var identitySet, out error))
        {
            return false;
        }

        var claims = ResolveBrokerClaims(principal, options, identitySet.PrimaryIdentity);
        profile = new AuthenticatedUserProfile(
            identitySet.PrimaryIdentity,
            identitySet.Identities,
            claims.HandleSeed,
            claims.Email,
            claims.DisplayName,
            claims.AvatarUrl);
        return true;
    }
}
