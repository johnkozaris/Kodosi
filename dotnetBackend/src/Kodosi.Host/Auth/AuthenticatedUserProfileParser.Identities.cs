using System.Security.Claims;
using Kodosi.Application;
using Kodosi.Host.Configuration;

namespace Kodosi.Host.Auth;

public static partial class AuthenticatedUserProfileParser
{
    private static bool TryResolveBrokerIdentitySet(
        ClaimsPrincipal principal,
        ExternalAuthProviderOptions options,
        out ResolvedAuthenticatedIdentitySet identitySet,
        out string? error)
    {
        identitySet = default!;
        error = null;

        var brokerSubject = principal.FindFirstValue(options.SubjectClaim)?.Trim();
        if (string.IsNullOrWhiteSpace(brokerSubject))
        {
            error = "Token is missing the required subject claim.";
            return false;
        }

        var brokerIdentity = new AuthenticatedExternalIdentity(
            options.Provider,
            ResolveCanonicalIssuer(principal, options),
            brokerSubject);
        var linkedIdentities = ResolveLinkedIdentities(principal, options, brokerIdentity);
        var primaryIdentity = linkedIdentities.FirstOrDefault() ?? brokerIdentity;
        var identities = new List<AuthenticatedExternalIdentity> { primaryIdentity };

        foreach (var linkedIdentity in linkedIdentities.Where(identity =>
                     !string.Equals(identity.IdentityKey, primaryIdentity.IdentityKey, StringComparison.Ordinal)))
        {
            identities.Add(linkedIdentity);
        }

        if (!identities.Any(identity => string.Equals(
                identity.IdentityKey,
                brokerIdentity.IdentityKey,
                StringComparison.Ordinal)))
        {
            identities.Add(brokerIdentity);
        }

        identitySet = new ResolvedAuthenticatedIdentitySet(primaryIdentity, identities);
        return true;
    }

    private static AuthenticatedExternalIdentity[] ResolveLinkedIdentities(
        ClaimsPrincipal principal,
        ExternalAuthProviderOptions options,
        AuthenticatedExternalIdentity brokerIdentity)
    {
        return options.LinkedIdentityClaims
            .Select(linked => principal.FindFirstValue(linked.SubjectClaim)?.Trim() is { Length: > 0 } subject
                ? new AuthenticatedExternalIdentity(linked.Provider, linked.Issuer.Trim(), subject)
                : null)
            .Where(identity => identity is not null)
            .Cast<AuthenticatedExternalIdentity>()
            .Where(identity => !string.Equals(
                identity.IdentityKey,
                brokerIdentity.IdentityKey,
                StringComparison.Ordinal))
            .GroupBy(identity => identity.IdentityKey, StringComparer.Ordinal)
            .Select(group => group.First())
            .ToArray();
    }

    private static string ResolveCanonicalIssuer(ClaimsPrincipal principal, ExternalAuthProviderOptions options)
    {
        if (!string.IsNullOrWhiteSpace(options.CanonicalIssuer))
        {
            return options.CanonicalIssuer.Trim();
        }

        var issuer = principal.FindFirstValue("iss")?.Trim();
        if (!string.IsNullOrWhiteSpace(issuer))
        {
            return issuer;
        }

        return options.ValidIssuers.FirstOrDefault(static issuer => !string.IsNullOrWhiteSpace(issuer))?.Trim()
            ?? options.Authority.Trim().TrimEnd('/');
    }

    private sealed record ResolvedAuthenticatedIdentitySet(
        AuthenticatedExternalIdentity PrimaryIdentity,
        IReadOnlyList<AuthenticatedExternalIdentity> Identities);
}
