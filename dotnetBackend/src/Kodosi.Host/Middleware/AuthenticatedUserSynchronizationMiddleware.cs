using Kodosi.Application;
using Kodosi.Domain;
using System.Security.Claims;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Options;
using Kodosi.Host.Auth;
using Kodosi.Host.Configuration;

namespace Kodosi.Host.Middleware;

public sealed class AuthenticatedUserSynchronizationMiddleware(
    RequestDelegate next,
    IServiceScopeFactory scopeFactory)
{
    private readonly RequestDelegate _next = next;
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;

    public async Task InvokeAsync(
        HttpContext context,
        AuthenticatedUserSyncCache syncCache,
        AuthenticatedUserProvisioningLock provisioningLock,
        IOptions<OidcAuthOptions> authOptions)
    {
        try
        {
            if (context.User.Identity?.IsAuthenticated == true &&
                context.Features.Get<AuthenticatedProfileFeature>() is { Profile: var profile })
            {
                var ttl = TimeSpan.FromSeconds(authOptions.Value.LocalProfileSyncTtlSeconds);
                var now = DateTimeOffset.UtcNow;

                if (context.User.Identity is ClaimsIdentity identity)
                {
                    if (syncCache.TryGetFresh(profile, ttl, now, out var cached))
                    {
                        StampLocalClaims(identity, cached.UserId, cached.Handle);
                    }
                    else
                    {
                        await using var provisioningLease =
                            await provisioningLock.AcquireAsync(
                                profile.Identities.Select(identity => identity.IdentityKey),
                                context.RequestAborted);
                        if (syncCache.TryGetFresh(profile, ttl, now, out cached))
                        {
                            StampLocalClaims(identity, cached.UserId, cached.Handle);
                        }
                        else
                        {
                            var user = await ResolveOrProvisionAsync(
                                profile,
                                context.RequestAborted);
                            syncCache.RecordSync(profile, user.UserId, user.Handle, now, ttl);
                            StampLocalClaims(
                                identity,
                                user.UserId.Value,
                                user.Handle);
                        }
                    }
                }
            }
        }
        finally
        {


            context.Features.Set<AuthenticatedProfileFeature?>(null);
        }

        await _next(context);
    }

    private async Task<LocalUserProfile> ResolveOrProvisionAsync(
        AuthenticatedUserProfile profile,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var users = scope.ServiceProvider.GetRequiredService<UserIdentityService>();
        var user = await users.ResolveOrProvisionAsync(profile, ct);
        return new LocalUserProfile(user.Id, user.Handle);
    }

    private static void StampLocalClaims(
        ClaimsIdentity identity,
        Guid userId,
        string handle)
    {
        SetSingleClaim(identity, AuthClaimTypes.UserId, userId.ToString());
        SetSingleClaim(identity, ClaimTypes.Name, handle);
    }

    private static void SetSingleClaim(ClaimsIdentity identity, string claimType, string value)
    {
        foreach (var existing in identity.FindAll(claimType).ToArray())
        {
            identity.RemoveClaim(existing);
        }

        identity.AddClaim(new Claim(claimType, value));
    }

    private readonly record struct LocalUserProfile(UserId UserId, string Handle);
}
