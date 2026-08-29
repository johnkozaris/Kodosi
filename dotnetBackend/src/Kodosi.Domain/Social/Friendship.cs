
namespace Kodosi.Domain;

public sealed class Friendship
{
    public UserId UserLowId { get; private set; }
    public UserId UserHighId { get; private set; }
    public UserId RequestorUserId { get; private set; }
    public FriendshipStatus Status { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }
    public DateTimeOffset? AcceptedAt { get; private set; }

    public uint Version { get; private set; }

    private Friendship() { }

    public static Friendship CreateRequest(UserId requestorId, UserId targetUserId)
    {
        if (requestorId == targetUserId)
        {
            throw new DomainException("Cannot send a friend request to yourself.");
        }

        var (low, high) = OrderPair(requestorId, targetUserId);
        return new Friendship
        {
            UserLowId = low,
            UserHighId = high,
            RequestorUserId = requestorId,
            Status = FriendshipStatus.Pending,
            CreatedAt = DateTimeOffset.UtcNow,
        };
    }

    private bool Includes(UserId userId)
        => UserLowId == userId || UserHighId == userId;

    public bool IsRequestedBy(UserId userId)
        => RequestorUserId == userId;

    public UserId GetOtherUserId(UserId userId)
    {
        if (UserLowId == userId)
        {
            return UserHighId;
        }

        if (UserHighId == userId)
        {
            return UserLowId;
        }

        throw new DomainException("User is not part of this friendship.");
    }

    public void Accept(UserId acceptingUserId)
    {
        if (Status != FriendshipStatus.Pending)
        {
            throw new InvalidStateException("Only pending friend requests can be accepted.");
        }

        if (!Includes(acceptingUserId))
        {
            throw new DomainException("Only participants can accept a friend request.");
        }

        if (RequestorUserId == acceptingUserId)
        {
            throw new DomainException("The requestor cannot accept their own friend request.");
        }

        Status = FriendshipStatus.Accepted;
        AcceptedAt = DateTimeOffset.UtcNow;
    }

    private static (UserId Low, UserId High) OrderPair(UserId a, UserId b)
    {
        return a.Value.CompareTo(b.Value) <= 0 ? (a, b) : (b, a);
    }
}
