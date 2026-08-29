namespace Kodosi.Domain;

public sealed class SessionViewerDismissal
{
    public SessionId SessionId { get; private set; }
    public UserId ViewerUserId { get; private set; }

    private SessionViewerDismissal() { }

    public static SessionViewerDismissal Create(
        SessionId sessionId,
        UserId viewerUserId)
    {
        if (sessionId.Value == Guid.Empty)
        {
            throw new DomainException("Session ID cannot be empty.");
        }

        if (viewerUserId.Value == Guid.Empty)
        {
            throw new DomainException("Viewer user ID cannot be empty.");
        }

        return new SessionViewerDismissal
        {
            SessionId = sessionId,
            ViewerUserId = viewerUserId,
        };
    }
}
