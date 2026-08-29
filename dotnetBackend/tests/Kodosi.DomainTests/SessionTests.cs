using System.Reflection;
using Kodosi.Domain;

namespace Kodosi.DomainTests;

public class SessionTests
{
    [Fact]
    public void Create_WithRoomScope_RequiresRoomId()
    {
        Assert.Throws<DomainException>(() =>
            Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
                UserId.New(),
                "Test",
                SessionScope.Room,
                ToolKind.ClaudeCode,
                AccessLevel.Suggest,
                "secret-hash",
                roomId: null));
    }

    [Fact]
    public void Create_WithInjectDefault_Rejects_Invalid_DefaultAccess()
    {
        Assert.Throws<DomainException>(() => Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.Inject,
            "secret-hash"));
    }

    [Theory]
    [InlineData(SessionScope.JustMe, AccessLevel.Inject)]
    [InlineData(SessionScope.JustMe, AccessLevel.Approve)]
    [InlineData(SessionScope.MyDevices, AccessLevel.Inject)]
    [InlineData(SessionScope.MyDevices, AccessLevel.Approve)]
    [InlineData(SessionScope.Friends, AccessLevel.Inject)]
    [InlineData(SessionScope.Friends, AccessLevel.Approve)]
    [InlineData(SessionScope.Room, AccessLevel.Inject)]
    [InlineData(SessionScope.Room, AccessLevel.Approve)]
    public void Create_Rejects_OwnerOnly_DefaultAccess_For_Every_Scope(
        SessionScope scope,
        AccessLevel defaultAccess)
    {
        Assert.Throws<DomainException>(() => Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            scope,
            ToolKind.ClaudeCode,
            defaultAccess,
            "secret-hash",
            scope == SessionScope.Room ? RoomId.From(Guid.NewGuid()) : null));
    }

    [Theory]
    [InlineData(SessionScope.JustMe, AccessLevel.View)]
    [InlineData(SessionScope.JustMe, AccessLevel.Suggest)]
    [InlineData(SessionScope.MyDevices, AccessLevel.View)]
    [InlineData(SessionScope.MyDevices, AccessLevel.Suggest)]
    [InlineData(SessionScope.Friends, AccessLevel.View)]
    [InlineData(SessionScope.Friends, AccessLevel.Suggest)]
    [InlineData(SessionScope.Room, AccessLevel.View)]
    [InlineData(SessionScope.Room, AccessLevel.Suggest)]
    public void Create_Accepts_Audience_DefaultAccess_For_Every_Scope(
        SessionScope scope,
        AccessLevel defaultAccess)
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            scope,
            ToolKind.ClaudeCode,
            defaultAccess,
            "secret-hash",
            scope == SessionScope.Room ? RoomId.From(Guid.NewGuid()) : null);

        Assert.Equal(scope, session.Scope);
        Assert.Equal(defaultAccess, session.DefaultAccess);
    }

    [Fact]
    public void GoLive_TransitionsToLive()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.Suggest,
            "secret-hash");

        session.ActivateHost("test-host");

        session.ReleaseHostSlot("test-host");

        Assert.Equal(SessionStatus.Live, session.Status);
    }

    [Fact]
    public void End_PreventsSubsequentStateChanges()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.Suggest,
            "secret-hash");

        session.End();

        Assert.Throws<DomainException>(() => session.ActivateHost("host"));
    }

    [Fact]
    public void FenceKeyPublication_Advances_Generation()
    {
        var session = CreateSession();

        session.FenceKeyPublication();

        Assert.Equal(1, session.CurrentKeyGeneration);
    }

    [Fact]
    public void FenceKeyPublication_Rejects_Ended_Session_Without_Mutation()
    {
        var session = CreateSession();
        SetCurrentKeyGeneration(session, 7);
        session.End();

        Assert.Throws<DomainException>(() => session.FenceKeyPublication());

        Assert.Equal(7, session.CurrentKeyGeneration);
    }

    [Fact]
    public void FenceKeyPublication_Rejects_Exhausted_Generation_Without_Mutation()
    {
        var session = CreateSession();
        SetCurrentKeyGeneration(session, int.MaxValue);

        Assert.Throws<DomainException>(() => session.FenceKeyPublication());

        Assert.Equal(int.MaxValue, session.CurrentKeyGeneration);
    }

    [Theory]
    [InlineData(SessionScope.Friends)]
    [InlineData(SessionScope.JustMe)]
    public void UpdateDefaultAccess_Rejects_Inject_Outside_Allowed_Scope(SessionScope scope)
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            scope,
            ToolKind.ClaudeCode,
            AccessLevel.View,
            "secret-hash");

        session.ActivateHost("test-host");

        session.ReleaseHostSlot("test-host");
        Assert.Throws<DomainException>(() => session.UpdateDefaultAccess(AccessLevel.Inject));
    }

    [Fact]
    public void UpdateScope_DowngradesLegacyInjectDefault()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            SessionScope.JustMe,
            ToolKind.ClaudeCode,
            AccessLevel.Suggest,
            "secret-hash");
        SetDefaultAccess(session, AccessLevel.Inject);

        session.UpdateScope(SessionScope.Friends, roomId: null);

        Assert.Equal(AccessLevel.Suggest, session.DefaultAccess);
    }

    [Fact]
    public void UpdateTitle_ChangesTitle()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.Suggest,
            "secret-hash");

        session.UpdateTitle("  Renamed Session  ");

        Assert.Equal("Renamed Session", session.Title);
    }

    [Fact]
    public void UpdateTitle_RejectsBlank()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.Suggest,
            "secret-hash");

        Assert.Throws<DomainException>(() => session.UpdateTitle("   "));
    }

    [Fact]
    public void GoLive_AcceptsReconnectingState()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.Suggest,
            "secret-hash");

        session.ActivateHost("test-host");

        session.ReleaseHostSlot("test-host");
        session.MarkReconnecting();

        Assert.Equal(SessionStatus.Reconnecting, session.Status);

        session.ActivateHost("test-host");

        session.ReleaseHostSlot("test-host");

        Assert.Equal(SessionStatus.Live, session.Status);
    }

    [Fact]
    public void MarkReconnecting_RejectsPendingState()
    {
        var session = Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.Suggest,
            "secret-hash");

        Assert.Throws<DomainException>(() => session.MarkReconnecting());
    }

    private static void SetDefaultAccess(Session session, AccessLevel access)
    {
        typeof(Session)
            .GetProperty(nameof(Session.DefaultAccess), BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .SetValue(session, access);
    }

    private static void SetCurrentKeyGeneration(Session session, int generation)
    {
        typeof(Session)
            .GetProperty(
                nameof(Session.CurrentKeyGeneration),
                BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .SetValue(session, generation);
    }

    private static Session CreateSession() =>
        Session.Create(SessionId.New(), Guid.CreateVersion7(), 1, Session.CurrentIncarnationProtocolVersion,
            UserId.New(),
            "Test",
            SessionScope.Friends,
            ToolKind.ClaudeCode,
            AccessLevel.Suggest,
            "secret-hash");
}
