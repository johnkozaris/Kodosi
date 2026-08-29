using Kodosi.Application;

namespace Kodosi.Host.Auth;

internal sealed class AuthenticatedProfileFeature(AuthenticatedUserProfile profile)
{
    public AuthenticatedUserProfile Profile { get; } = profile;
}
