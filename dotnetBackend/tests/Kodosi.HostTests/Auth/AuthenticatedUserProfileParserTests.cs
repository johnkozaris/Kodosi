using System.Security.Claims;
using Kodosi.Host.Auth;
using Kodosi.Host.Configuration;

namespace Kodosi.HostTests;

public sealed class AuthenticatedUserProfileParserTests
{
    [Fact]
    public void TryParse_Deduplicates_Canonical_Broker_And_Linked_Identity()
    {
        var principal = new ClaimsPrincipal(new ClaimsIdentity(
            [
                new Claim("sub", "subject"),
                new Claim("iss", "https://issuer.test"),
                new Claim("linked_subject", "subject"),
            ],
            authenticationType: "broker"));
        var options = new ExternalAuthProviderOptions
        {
            Scheme = "broker",
            Provider = "broker",
            Authority = "https://issuer.test",
            CanonicalIssuer = "https://issuer.test",
            Audiences = ["kodosi"],
            LinkedIdentityClaims =
            [
                new LinkedIdentityClaimOptions
                {
                    Provider = " BROKER ",
                    Issuer = " https://issuer.test ",
                    SubjectClaim = "linked_subject",
                },
            ],
        };

        var parsed = AuthenticatedUserProfileParser.TryParse(principal, options, out var profile, out var error);

        Assert.True(parsed);
        Assert.Null(error);
        Assert.Single(profile.Identities);
        Assert.Equal(profile.PrimaryIdentity.IdentityKey, profile.Identities[0].IdentityKey);
    }

    [Fact]
    public void TryParse_Uses_Linked_Apple_Identity_As_Primary_For_Authentik_Tokens()
    {
        var principal = new ClaimsPrincipal(new ClaimsIdentity(
            [
                new Claim("sub", "authentik-sub-1"),
                new Claim("iss", "https://auth.kodosi.com/application/o/kodosi/"),
                new Claim("email", "john@example.com"),
                new Claim("name", "John Example"),
                new Claim("preferred_username", "john"),
                new Claim("kodosi_apple_subject", "apple-sub-1"),
            ],
            authenticationType: "authentik"));
        var options = new ExternalAuthProviderOptions
        {
            Scheme = "authentik",
            Provider = "authentik",
            Authority = "https://auth.kodosi.com/application/o/kodosi/",
            CanonicalIssuer = "https://auth.kodosi.com/application/o/kodosi/",
            Audiences = ["kodosi-app"],
            LinkedIdentityClaims =
            [
                new LinkedIdentityClaimOptions
                {
                    Provider = "apple",
                    Issuer = "https://appleid.apple.com",
                    SubjectClaim = "kodosi_apple_subject",
                },
            ],
        };

        var parsed = AuthenticatedUserProfileParser.TryParse(principal, options, out var profile, out var error);

        Assert.True(parsed);
        Assert.Null(error);
        Assert.Equal("apple", profile.PrimaryIdentity.Provider);
        Assert.Collection(
            profile.Identities,
            identity =>
            {
                Assert.Equal("apple", identity.Provider);
                Assert.Equal("https://appleid.apple.com", identity.Issuer);
                Assert.Equal("apple-sub-1", identity.Subject);
            },
            identity =>
            {
                Assert.Equal("authentik", identity.Provider);
                Assert.Equal("authentik-sub-1", identity.Subject);
            });
        Assert.Equal("john", profile.HandleSeed);
        Assert.Equal("john@example.com", profile.Email);
        Assert.Equal("John Example", profile.DisplayName);
    }

    [Fact]
    public void TryParse_Direct_Apple_Profile_Uses_Only_Token_Claims()
    {
        var principal = new ClaimsPrincipal(new ClaimsIdentity(
            [
                new Claim("sub", "apple-sub-1"),
                new Claim("iss", "https://appleid.apple.com"),
            ],
            authenticationType: "apple"));
        var options = new ExternalAuthProviderOptions
        {
            Scheme = "apple",
            Provider = "apple",
            Authority = "https://appleid.apple.com",
            CanonicalIssuer = "https://appleid.apple.com",
            Audiences = ["com.kodosi.mobile"],
        };

        var parsed = AuthenticatedUserProfileParser.TryParse(principal, options, out var profile, out var error);

        Assert.True(parsed);
        Assert.Null(error);
        Assert.Null(profile.Email);
        Assert.Null(profile.DisplayName);
        Assert.StartsWith("apple-", profile.HandleSeed);
        Assert.Equal(18, profile.HandleSeed.Length);
    }
}
