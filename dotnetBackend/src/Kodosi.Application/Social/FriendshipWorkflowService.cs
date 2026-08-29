using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class FriendshipWorkflowService(
    UserService users,
    SessionAccessOverrideRevoker accessRevoker,
    IFriendshipAuditRepository audit,
    IUserLifecycleLock userLifecycleLock,
    ISessionEndAuthority sessionEndAuthority,
    IUnitOfWork unitOfWork)
{
    private readonly UserService _users = users;
    private readonly SessionAccessOverrideRevoker _accessRevoker = accessRevoker;
    private readonly IFriendshipAuditRepository _audit = audit;
    private readonly IUserLifecycleLock _userLifecycleLock = userLifecycleLock;
    private readonly ISessionEndAuthority _sessionEndAuthority = sessionEndAuthority;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<UserId> SendRequestAsync(
        UserId currentUserId,
        string username,
        RequestAuditContext auditContext,
        CancellationToken ct = default)
    {
        await using var tx = await _unitOfWork.BeginTransactionAsync(ct);
        var targetUserId = await _users.SendFriendRequestAsync(currentUserId, username, ct);
        await AddAuditAsync(currentUserId, targetUserId, FriendshipAuditAction.RequestSent, auditContext, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await tx.CommitAsync(ct);
        return targetUserId;
    }

    public async Task<UserId> AcceptRequestAsync(
        UserId currentUserId,
        string username,
        RequestAuditContext auditContext,
        CancellationToken ct = default)
    {
        await using var tx = await _unitOfWork.BeginTransactionAsync(ct);
        var friendUserId = await _users.AcceptFriendRequestAsync(currentUserId, username, ct);
        await AddAuditAsync(currentUserId, friendUserId, FriendshipAuditAction.RequestAccepted, auditContext, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await tx.CommitAsync(ct);
        return friendUserId;
    }

    public async Task<UserId> RejectRequestAsync(
        UserId currentUserId,
        string username,
        RequestAuditContext auditContext,
        CancellationToken ct = default)
    {
        await using var tx = await _unitOfWork.BeginTransactionAsync(ct);
        var otherUserId = await _users.RejectFriendRequestAsync(currentUserId, username, ct);
        await AddAuditAsync(currentUserId, otherUserId, FriendshipAuditAction.RequestRejected, auditContext, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await tx.CommitAsync(ct);
        return otherUserId;
    }

    public async Task<UserId> CancelOutgoingRequestAsync(
        UserId currentUserId,
        string handle,
        RequestAuditContext auditContext,
        CancellationToken ct = default)
    {
        await using var tx = await _unitOfWork.BeginTransactionAsync(ct);
        var otherUserId = await _users.CancelOutgoingFriendRequestAsync(currentUserId, handle, ct);
        await AddAuditAsync(currentUserId, otherUserId, FriendshipAuditAction.RequestCancelled, auditContext, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await tx.CommitAsync(ct);
        return otherUserId;
    }

    public async Task<FriendshipRemovalResult> RemoveFriendshipAsync(
        UserId currentUserId,
        string handle,
        RequestAuditContext auditContext,
        CancellationToken ct = default)
    {
        ct.ThrowIfCancellationRequested();
        var authorization = await _users.AuthorizeFriendshipRemovalAsync(
            currentUserId,
            handle,
            ct);
        ct.ThrowIfCancellationRequested();

        await using var tx = await _unitOfWork.BeginTransactionAsync(ct);

        var otherUserId = authorization.OtherUserId;
        foreach (var userId in new[] { currentUserId, otherUserId }
            .Distinct()
            .OrderBy(userId => userId.Value))
        {
            await _userLifecycleLock.AcquireAsync(userId, ct);
        }

        var noCancellation = CancellationToken.None;
        await _users.RemoveFriendshipAsync(
            currentUserId,
            otherUserId,
            authorization.OtherHandle,
            noCancellation);
        var ownerSessionIds =
            await _accessRevoker.GetNonEndedFriendSessionIdsByOwnerAsync(
                currentUserId,
                noCancellation);
        var formerFriendSessionIds =
            await _accessRevoker.GetNonEndedFriendSessionIdsByOwnerAsync(
                otherUserId,
                noCancellation);
        var sessionLifecycles = await _sessionEndAuthority.AcquireAsync(
            ownerSessionIds.Concat(formerFriendSessionIds).ToList(),
            noCancellation);
        try
        {
            var ownerSessions = await _accessRevoker.RevokeFriendshipOverridesAsync(
                currentUserId,
                otherUserId,
                noCancellation);
            var formerFriendSessions = await _accessRevoker.RevokeFriendshipOverridesAsync(
                otherUserId,
                currentUserId,
                noCancellation);

            await AddAuditAsync(
                currentUserId,
                otherUserId,
                FriendshipAuditAction.Removed,
                auditContext,
                noCancellation);
            await _unitOfWork.SaveChangesAsync(noCancellation);
            await tx.CommitAsync(noCancellation);

            return new FriendshipRemovalResult(
                otherUserId,
                ownerSessions,
                formerFriendSessions,
                sessionLifecycles);
        }
        catch
        {
            await sessionLifecycles.DisposeAsync();
            throw;
        }
    }

    private Task AddAuditAsync(
        UserId actorUserId,
        UserId otherUserId,
        FriendshipAuditAction action,
        RequestAuditContext auditContext,
        CancellationToken ct)
        => _audit.AddAsync(
            FriendshipAuditEntry.Create(
                actorUserId,
                otherUserId,
                action,
                auditContext.ClientIp,
                auditContext.UserAgent),
            ct);
}

public sealed record FriendshipRemovalResult(
    UserId OtherUserId,
    IReadOnlyList<SessionAccessFanoutTarget> OwnerSessions,
    IReadOnlyList<SessionAccessFanoutTarget> FormerFriendSessions,
    IAsyncDisposable? Lifecycle = null) : IAsyncDisposable
{
    public ValueTask DisposeAsync() =>
        Lifecycle?.DisposeAsync() ?? ValueTask.CompletedTask;
}
