using System.Security.Claims;
using Kodosi.Domain;

namespace Kodosi.Host.Auth;

public sealed class AuthenticatedCurrentUser(IHttpContextAccessor httpContextAccessor) : ICurrentUser
{
    private readonly IHttpContextAccessor _httpContextAccessor = httpContextAccessor;

    public UserId UserId
    {
        get
        {
            EnsureAuthenticated();

            var rawUserId = _httpContextAccessor.HttpContext!.User.FindFirstValue(AuthClaimTypes.UserId);
            if (!Guid.TryParse(rawUserId, out var userId))
            {
                throw new InvalidOperationException("Authenticated request is missing the canonical user id claim.");
            }

            return UserId.From(userId);
        }
    }

    private void EnsureAuthenticated()
    {
        if (_httpContextAccessor.HttpContext?.User.Identity?.IsAuthenticated != true)
        {
            throw new InvalidOperationException("The current request is not authenticated.");
        }
    }
}
