using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class DeviceListAudienceTests
{
    [Fact]
    public void Build_Unions_Friends_Room_Peers_And_Self()
    {
        var self = UserId.New();
        var friend = UserId.New();
        var roomPeer = UserId.New();

        var audience = DeviceListAudience.Build(
            friendIds: [friend],
            roomPeerIds: [roomPeer],
            selfUserId: self);

        Assert.Contains(self, audience);
        Assert.Contains(friend, audience);
        Assert.Contains(roomPeer, audience);
        Assert.Equal(3, audience.Count);
    }

    [Fact]
    public void Build_Deduplicates_Users_Appearing_In_Multiple_Tiers()
    {
        var self = UserId.New();
        var shared = UserId.New();
        var friendOnly = UserId.New();

        var audience = DeviceListAudience.Build(
            friendIds: [shared, friendOnly],
            roomPeerIds: [shared],
            selfUserId: self);

        Assert.Equal(3, audience.Count);
        Assert.Single(audience, id => id == shared);
    }

    [Fact]
    public void Build_Deduplicates_Self_When_Upstream_Repo_Leaks_It()
    {
        var self = UserId.New();

        var audience = DeviceListAudience.Build(
            friendIds: [self],
            roomPeerIds: [self],
            selfUserId: self);

        Assert.Single(audience);
    }
}
