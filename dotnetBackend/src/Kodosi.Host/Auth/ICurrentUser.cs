using Kodosi.Domain;

namespace Kodosi.Host.Auth;

public interface ICurrentUser
{
    UserId UserId { get; }
}
