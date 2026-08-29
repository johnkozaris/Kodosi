using Kodosi.Domain;

namespace Kodosi.DomainTests;

public class FriendshipTests
{
    [Fact]
    public void CreateRequest_Orders_UserIds_Canonically_Regardless_Of_Argument_Order()
    {
        var requestor = UserId.From(Guid.Parse("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb"));
        var target = UserId.From(Guid.Parse("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"));

        var friendship = Friendship.CreateRequest(requestor, target);

        Assert.Equal(target, friendship.UserLowId);
        Assert.Equal(requestor, friendship.UserHighId);
    }

    [Fact]
    public void CreateRequest_RejectsSelfFriendRequest()
    {
        var userId = UserId.New();

        Assert.Throws<DomainException>(() => Friendship.CreateRequest(userId, userId));
    }

    [Fact]
    public void Accept_ByRequestedUser_TransitionsToAccepted()
    {
        var requestor = UserId.New();
        var target = UserId.New();
        var friendship = Friendship.CreateRequest(requestor, target);

        friendship.Accept(target);

        Assert.Equal(FriendshipStatus.Accepted, friendship.Status);
        Assert.NotNull(friendship.AcceptedAt);
    }

    [Fact]
    public void Accept_ByRequestor_IsRejected()
    {
        var requestor = UserId.New();
        var target = UserId.New();
        var friendship = Friendship.CreateRequest(requestor, target);

        Assert.Throws<DomainException>(() => friendship.Accept(requestor));
    }
}
