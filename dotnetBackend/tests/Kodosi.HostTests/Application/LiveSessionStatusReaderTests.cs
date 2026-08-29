using Kodosi.Application;
using Kodosi.Domain;
using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class LiveSessionStatusReaderTests
{
    [Fact]
    public async Task GetStatusAsync_ReturnsPersistedStatus()
    {
        var session = CreateSession(UserId.New(), SessionScope.Friends);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var repository = new FakeSessionRepository(session);
        var service = new LiveSessionStatusReader(repository);

        var status = await service.GetStatusAsync(session.Id, TestContext.Current.CancellationToken);

        Assert.Equal(SessionStatus.Live, status);
    }
}
