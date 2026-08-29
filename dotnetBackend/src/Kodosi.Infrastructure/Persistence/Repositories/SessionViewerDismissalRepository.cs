using Kodosi.Application;
using Kodosi.Domain;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Infrastructure.Persistence.Repositories;

public sealed class SessionViewerDismissalRepository(KodosiDbContext context)
    : ISessionViewerDismissalRepository
{
    private readonly KodosiDbContext _context = context;

    public Task<bool> ExistsAsync(
        SessionId sessionId,
        UserId viewerUserId,
        CancellationToken ct = default)
        => _context.SessionViewerDismissals
            .AsNoTracking()
            .AnyAsync(
                dismissal => dismissal.SessionId == sessionId
                    && dismissal.ViewerUserId == viewerUserId,
                ct);

    public async Task<IReadOnlySet<UserId>> GetDismissedViewerIdsAsync(
        IReadOnlyCollection<SessionId> sessionIds,
        IReadOnlyCollection<UserId> viewerUserIds,
        CancellationToken ct = default)
    {
        if (sessionIds.Count == 0 || viewerUserIds.Count == 0)
        {
            return new HashSet<UserId>();
        }

        return await _context.SessionViewerDismissals
            .AsNoTracking()
            .Where(dismissal =>
                sessionIds.Contains(dismissal.SessionId)
                && viewerUserIds.Contains(dismissal.ViewerUserId))
            .Select(dismissal => dismissal.ViewerUserId)
            .ToHashSetAsync(ct);
    }

    public async Task<IReadOnlySet<SessionId>> GetDismissedSessionIdsAsync(
        UserId viewerUserId,
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct = default)
    {
        if (sessionIds.Count == 0)
        {
            return new HashSet<SessionId>();
        }

        return await _context.SessionViewerDismissals
            .AsNoTracking()
            .Where(dismissal =>
                dismissal.ViewerUserId == viewerUserId
                && sessionIds.Contains(dismissal.SessionId))
            .Select(dismissal => dismissal.SessionId)
            .ToHashSetAsync(ct);
    }

    public async Task<bool> AddIfAbsentAsync(
        SessionViewerDismissal dismissal,
        CancellationToken ct = default)
    {
        var inserted = await _context.Database.ExecuteSqlInterpolatedAsync(
            $"""
             INSERT INTO session_viewer_dismissals (session_id, viewer_user_id)
             VALUES ({dismissal.SessionId.Value}, {dismissal.ViewerUserId.Value})
             ON CONFLICT (session_id, viewer_user_id) DO NOTHING
             """,
            ct);
        return inserted == 1;
    }

    public Task DeleteForSessionAsync(
        SessionId sessionId,
        CancellationToken ct = default) =>
        _context.SessionViewerDismissals
            .Where(dismissal => dismissal.SessionId == sessionId)
            .ExecuteDeleteAsync(ct);
}
