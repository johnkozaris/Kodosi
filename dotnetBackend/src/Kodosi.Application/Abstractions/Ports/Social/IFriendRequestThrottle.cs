using Kodosi.Domain;

namespace Kodosi.Application;



public interface IFriendRequestThrottle
{
    TimeSpan? TryClaim(UserId senderId, UserId targetId);

    void Release(UserId senderId, UserId targetId);

    void Sweep();
}
