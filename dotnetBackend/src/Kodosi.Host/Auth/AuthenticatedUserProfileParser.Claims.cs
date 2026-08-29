using System.Security.Claims;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Application;
using Kodosi.Host.Configuration;

namespace Kodosi.Host.Auth;

public static partial class AuthenticatedUserProfileParser
{
    private static ParsedAuthenticatedUserProfileClaims ResolveBrokerClaims(
        ClaimsPrincipal principal,
        ExternalAuthProviderOptions options,
        AuthenticatedExternalIdentity primaryIdentity)
    {
        var email = TrimOrNull(principal.FindFirstValue(options.EmailClaim));
        var displayName = TrimOrNull(principal.FindFirstValue(options.DisplayNameClaim));
        return new ParsedAuthenticatedUserProfileClaims(
            ResolveHandleSeed(principal, options, email, displayName, primaryIdentity),
            email,
            displayName,
            NormalizeAvatarUrl(principal.FindFirstValue(options.AvatarUrlClaim)));
    }

    private static string ResolveHandleSeed(
        ClaimsPrincipal principal,
        ExternalAuthProviderOptions options,
        string? email,
        string? displayName,
        AuthenticatedExternalIdentity primaryIdentity)
    {
        return (
            principal.FindFirstValue(options.UserNameClaim)?.Trim() ??
            principal.FindFirstValue("preferred_username")?.Trim() ??
            principal.FindFirstValue("username")?.Trim() ??
            email?.Split('@', 2)[0] ??
            displayName ??
            BuildFallbackHandleSeed(primaryIdentity)).Trim();
    }

    private static string? NormalizeAvatarUrl(string? avatarUrl)
    {
        avatarUrl = TrimOrNull(avatarUrl);
        if (avatarUrl is not null && !Uri.TryCreate(avatarUrl, UriKind.Absolute, out _))
        {
            return null;
        }

        return avatarUrl;
    }

    private static string? TrimOrNull(string? value)
        => string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private static string BuildFallbackHandleSeed(AuthenticatedExternalIdentity identity)
    {
        var raw = $"{identity.Provider}\n{identity.Issuer}\n{identity.Subject}";
        var hash = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(raw)))[..12];
        return $"{identity.Provider}-{hash}";
    }

    private sealed record ParsedAuthenticatedUserProfileClaims(
        string HandleSeed,
        string? Email,
        string? DisplayName,
        string? AvatarUrl);
}
