using Kodosi.Application;
using Kodosi.Domain;

using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class SessionReaderTests
{
    [Fact]
    public async Task GetMySessions_Returns_Owned_Sessions_Including_Ended_With_Inject_Access()
    {
        var ownerId = UserId.New();
        var live = CreateSession(ownerId, SessionScope.JustMe);
        var ended = CreateSession(ownerId, SessionScope.Friends);
        live.ActivateHost("test-host");
        live.ReleaseHostSlot("test-host");
        ended.ActivateHost("test-host");
        ended.ReleaseHostSlot("test-host");
        ended.End();
        var sessions = new FakeSessionRepository(live);
        await sessions.AddAsync(ended, TestContext.Current.CancellationToken);

        var reader = CreateReader(sessions);

        var result = await reader.GetMySessionsAsync(ownerId, TestContext.Current.CancellationToken);

        Assert.Equal(2, result.Count);
        Assert.Contains(result, card => card.Id == live.Id.Value.ToString() && card.Status == SessionStatus.Live);
        Assert.Contains(result, card => card.Id == ended.Id.Value.ToString() && card.Status == SessionStatus.Ended);
        Assert.All(result, card =>
        {
            Assert.Equal(AccessLevel.Inject, card.Access);
            Assert.Equal(ownerId.Value.ToString(), card.OwnerUserId);
        });
    }

    [Fact]
    public async Task GetMySessions_Excludes_Sessions_Owned_By_Others()
    {
        var ownerId = UserId.New();
        var otherId = UserId.New();
        var mine = CreateSession(ownerId, SessionScope.JustMe);
        var theirs = CreateSession(otherId, SessionScope.Friends);
        theirs.ActivateHost("test-host");
        theirs.ReleaseHostSlot("test-host");
        var sessions = new FakeSessionRepository(mine);
        await sessions.AddAsync(theirs, TestContext.Current.CancellationToken);

        var reader = CreateReader(sessions);

        var result = await reader.GetMySessionsAsync(ownerId, TestContext.Current.CancellationToken);

        var card = Assert.Single(result);
        Assert.Equal(mine.Id.Value.ToString(), card.Id);
    }

    private static SessionReader CreateReader(FakeSessionRepository sessions)
        => new(sessions, CreateAccessService(), new FakeRuntimeDirectory());
}
