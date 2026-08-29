using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class ScopeOrderingTests
{
    [Fact]
    public void RequiresOverrideReconciliation_True_For_Room_To_Different_Room()
    {



        var roomA = SessionAudience.Of(SessionScope.Room, RoomId.From(Guid.NewGuid()));
        var roomB = SessionAudience.Of(SessionScope.Room, RoomId.From(Guid.NewGuid()));

        Assert.True(ScopeOrdering.RequiresOverrideReconciliation(roomA, roomB));
    }

    [Fact]
    public void RequiresOverrideReconciliation_False_For_Same_Room()
    {
        var roomId = RoomId.From(Guid.NewGuid());
        var audience = SessionAudience.Of(SessionScope.Room, roomId);

        Assert.False(ScopeOrdering.RequiresOverrideReconciliation(audience, audience));
        Assert.False(ScopeOrdering.RequiresOverrideReconciliation(
            audience,
            SessionAudience.Of(SessionScope.Room, roomId)));
    }

    [Fact]
    public void RequiresOverrideReconciliation_Audience_Still_Covers_Scope_Transitions()
    {
        var friends = SessionAudience.Of(SessionScope.Friends, null);
        var room = SessionAudience.Of(SessionScope.Room, RoomId.From(Guid.NewGuid()));

        Assert.True(ScopeOrdering.RequiresOverrideReconciliation(friends, room));
        Assert.True(ScopeOrdering.RequiresOverrideReconciliation(room, friends));
        Assert.True(ScopeOrdering.RequiresOverrideReconciliation(
            room,
            SessionAudience.Of(SessionScope.JustMe, null)));
        Assert.False(ScopeOrdering.RequiresOverrideReconciliation(
            SessionAudience.Of(SessionScope.JustMe, null),
            friends));
    }

    [Fact]
    public void SessionAudience_Drops_RoomId_For_NonRoom_Scopes()
    {


        Assert.Null(SessionAudience.Of(SessionScope.Friends, RoomId.From(Guid.NewGuid())).RoomId);
        Assert.Equal(
            SessionAudience.Of(SessionScope.Friends, null),
            SessionAudience.Of(SessionScope.Friends, RoomId.From(Guid.NewGuid())));
    }
}
