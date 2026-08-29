using Kodosi.Domain;

namespace Kodosi.DomainTests;

public class AccessResolverTests
{
    [Fact]
    public void Owner_Always_Gets_Inject()
    {
        var result = AccessResolver.ResolveAccess(
            isOwner: true,
            scope: SessionScope.JustMe,
            defaultAccess: AccessLevel.View,
            isFriend: false,
            isRoomMember: false,
            explicitOverride: null);

        Assert.Equal(AccessLevel.Inject, result);
    }

    [Fact]
    public void ExplicitOverride_Wins_Over_Default()
    {
        var result = AccessResolver.ResolveAccess(
            isOwner: false,
            scope: SessionScope.Friends,
            defaultAccess: AccessLevel.View,
            isFriend: true,
            isRoomMember: false,
            explicitOverride: AccessLevel.Inject);

        Assert.Equal(AccessLevel.Inject, result);
    }

    [Fact]
    public void Private_Session_Rejects_NonOwner()
    {
        Assert.Throws<PolicyViolationException>(() =>
            AccessResolver.ResolveAccess(
                isOwner: false,
                scope: SessionScope.JustMe,
                defaultAccess: AccessLevel.View,
                isFriend: false,
                isRoomMember: false,
                explicitOverride: null));
    }

    [Fact]
    public void Friends_Scope_Requires_Friendship()
    {
        Assert.Throws<PolicyViolationException>(() =>
            AccessResolver.ResolveAccess(
                isOwner: false,
                scope: SessionScope.Friends,
                defaultAccess: AccessLevel.Suggest,
                isFriend: false,
                isRoomMember: false,
                explicitOverride: null));
    }

    [Fact]
    public void Friends_Scope_Returns_DefaultAccess_For_Friends()
    {
        var result = AccessResolver.ResolveAccess(
            isOwner: false,
            scope: SessionScope.Friends,
            defaultAccess: AccessLevel.Suggest,
            isFriend: true,
            isRoomMember: false,
            explicitOverride: null);

        Assert.Equal(AccessLevel.Suggest, result);
    }

    [Fact]
    public void Room_Scope_Requires_Membership()
    {
        Assert.Throws<PolicyViolationException>(() =>
            AccessResolver.ResolveAccess(
                isOwner: false,
                scope: SessionScope.Room,
                defaultAccess: AccessLevel.Suggest,
                isFriend: false,
                isRoomMember: false,
                explicitOverride: null));
    }

    [Fact]
    public void TryResolveAccess_Returns_False_For_Denied_NonOwner()
    {
        var allowed = AccessResolver.TryResolveAccess(
            isOwner: false,
            scope: SessionScope.JustMe,
            defaultAccess: AccessLevel.View,
            isFriend: false,
            isRoomMember: false,
            explicitOverride: null,
            out var accessLevel);

        Assert.False(allowed);
        Assert.Equal(default, accessLevel);
    }

    [Fact]
    public void TryResolveAccess_Returns_AccessLevel_For_Allowed_User()
    {
        var allowed = AccessResolver.TryResolveAccess(
            isOwner: false,
            scope: SessionScope.Friends,
            defaultAccess: AccessLevel.Suggest,
            isFriend: true,
            isRoomMember: false,
            explicitOverride: null,
            out var accessLevel);

        Assert.True(allowed);
        Assert.Equal(AccessLevel.Suggest, accessLevel);
    }
}
