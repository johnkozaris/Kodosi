using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISessionViewerDismissalRepository
{
    Task<bool> ExistsAsync(
        SessionId sessionId,
        UserId viewerUserId,
        CancellationToken ct = default);

    Task<IReadOnlySet<UserId>> GetDismissedViewerIdsAsync(
        IReadOnlyCollection<SessionId> sessionIds,
        IReadOnlyCollection<UserId> viewerUserIds,
        CancellationToken ct = default);

    Task<IReadOnlySet<SessionId>> GetDismissedSessionIdsAsync(
        UserId viewerUserId,
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct = default);

    Task<bool> AddIfAbsentAsync(
        SessionViewerDismissal dismissal,
        CancellationToken ct = default);

    Task DeleteForSessionAsync(
        SessionId sessionId,
        CancellationToken ct = default);
}
