using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class SessionViewerDismissalTests
{
    [Fact]
    public void Create_Rejects_Empty_Session_Id()
    {
        var exception = Assert.Throws<DomainException>(() =>
            SessionViewerDismissal.Create(
                SessionId.From(Guid.Empty),
                UserId.New()));

        Assert.Equal("Session ID cannot be empty.", exception.Message);
    }

    [Fact]
    public void Create_Rejects_Empty_Viewer_User_Id()
    {
        var exception = Assert.Throws<DomainException>(() =>
            SessionViewerDismissal.Create(
                SessionId.New(),
                UserId.From(Guid.Empty)));

        Assert.Equal("Viewer user ID cannot be empty.", exception.Message);
    }
}
