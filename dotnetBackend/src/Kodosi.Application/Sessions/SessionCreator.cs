using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionCreator(
    ISessionRepository sessions,
    ISessionIncarnationRepository incarnations,
    IOwnerSessionSecretHasher ownerSessionSecretHasher,
    SessionRoomResolver roomResolver,
    SessionAccessOverrideRevoker accessOverrideRevoker,
    ISessionViewerDismissalRepository dismissals,
    ISessionKeyBlobRepository keyBlobs,
    IUserLifecycleLock userLifecycleLock,
    IRoomLifecycleLock roomLifecycleLock,
    ISessionEndAuthority sessionEndAuthority,
    IUnitOfWork unitOfWork)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly ISessionIncarnationRepository _incarnations = incarnations;
    private readonly IOwnerSessionSecretHasher _ownerSessionSecretHasher = ownerSessionSecretHasher;
    private readonly SessionRoomResolver _roomResolver = roomResolver;
    private readonly SessionAccessOverrideRevoker _accessOverrideRevoker = accessOverrideRevoker;
    private readonly ISessionViewerDismissalRepository _dismissals = dismissals;
    private readonly ISessionKeyBlobRepository _keyBlobs = keyBlobs;
    private readonly IUserLifecycleLock _userLifecycleLock = userLifecycleLock;
    private readonly IRoomLifecycleLock _roomLifecycleLock = roomLifecycleLock;
    private readonly ISessionEndAuthority _sessionEndAuthority = sessionEndAuthority;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<OwnedSessionCreateResult> CreateOwnedAsync(
        UserId ownerId,
        CreateSessionRequest request,
        CancellationToken ct = default)
    {
        var requestedSessionId = SessionId.From(request.Id);
        var secretHash = _ownerSessionSecretHasher.Hash(request.OwnerSecret);
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);

        await _userLifecycleLock.AcquireAsync(ownerId, ct);
        if (request.Scope == SessionScope.Room && request.RoomId.HasValue)
        {
            await _roomLifecycleLock.AcquireAsync(
                RoomId.From(request.RoomId.Value),
                ct);
        }
        await using var sessionLifecycle = await _sessionEndAuthority.AcquireAsync(
            [requestedSessionId],
            ct);
        var existing = await _sessions.GetByIdForUpdateAsync(
            requestedSessionId,
            ct);
        var idempotentIncarnation = await _incarnations.GetByIdempotencyKeyAsync(
            requestedSessionId,
            request.IdempotencyKey,
            ct);
        if (existing is not null)
        {
            if (existing.IsOwner(ownerId)
                && existing.Status != SessionStatus.Ended
                && idempotentIncarnation?.IncarnationId == existing.IncarnationId
                && _ownerSessionSecretHasher.Verify(
                    existing.OwnerSessionSecretHash,
                    request.OwnerSecret)
                && MatchesCreateRequest(existing, request))
            {
                await transaction.CommitAsync(ct);
                return new OwnedSessionCreateResult(
                    existing.ToDetailResponse(AccessLevel.Inject),
                    existing.ToDiscoveryTarget());
            }
            if (!existing.IsOwner(ownerId)
                || existing.Status != SessionStatus.Ended
                || idempotentIncarnation is not null)
            {
                throw new ConflictException($"Session '{request.Id}' already exists.");
            }
        }
        else if (idempotentIncarnation is not null)
        {
            throw new ConflictException(
                $"Session '{request.Id}' has inconsistent incarnation history.");
        }

        var roomId = await _roomResolver.ResolveAsync(
            ownerId,
            request.Scope,
            request.RoomId,
            ct);
        if (existing is not null)
        {
            var incarnation = NewIncarnation(
                requestedSessionId,
                checked(existing.IncarnationGeneration + 1),
                request.IdempotencyKey);
            await _sessionEndAuthority.RetireEndedIncarnationAsync(
                existing.ToDiscoveryTarget(),
                existing.StartedAt,
                ct);
            existing.Republish(
                incarnation.IncarnationId,
                incarnation.Generation,
                ownerId,
                request.Title,
                request.Scope,
                request.ToolKind,
                request.DefaultAccess.ToAccessLevel(),
                secretHash,
                roomId);
            await _accessOverrideRevoker.RevokeSessionOverridesAsync(requestedSessionId, ct);
            await _dismissals.DeleteForSessionAsync(requestedSessionId, ct);
            await _keyBlobs.DeleteForSessionAsync(requestedSessionId, ct);
            await _sessions.UpdateAsync(existing, ct);
            await _unitOfWork.SaveChangesAsync(ct);
            await _incarnations.AddAsync(incarnation, ct);
            await transaction.CommitAsync(ct);
            return new OwnedSessionCreateResult(
                existing.ToDetailResponse(AccessLevel.Inject),
                existing.ToDiscoveryTarget());
        }

        var initialIncarnation = NewIncarnation(
            requestedSessionId,
            generation: 1,
            request.IdempotencyKey);
        var session = Session.Create(
            requestedSessionId,
            initialIncarnation.IncarnationId,
            initialIncarnation.Generation,
            initialIncarnation.ProtocolVersion,
            ownerId,
            request.Title,
            request.Scope,
            request.ToolKind,
            request.DefaultAccess.ToAccessLevel(),
            secretHash,
            roomId);

        await _sessions.AddAsync(session, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await _incarnations.AddAsync(initialIncarnation, ct);
        await transaction.CommitAsync(ct);

        return new OwnedSessionCreateResult(
            session.ToDetailResponse(AccessLevel.Inject),
            session.ToDiscoveryTarget());
    }

    private static bool MatchesCreateRequest(
        Session session,
        CreateSessionRequest request) =>
        session.Title == request.Title
        && session.Scope == request.Scope
        && session.ToolKind == request.ToolKind
        && session.DefaultAccess == request.DefaultAccess.ToAccessLevel()
        && session.RoomId?.Value == (
            request.Scope == SessionScope.Room
                ? request.RoomId
                : null);

    private static SessionIncarnationRecord NewIncarnation(
        SessionId sessionId,
        long generation,
        Guid idempotencyKey) =>
        new(
            sessionId,
            generation,
            Guid.CreateVersion7(),
            Session.CurrentIncarnationProtocolVersion,
            idempotencyKey,
            DateTimeOffset.UtcNow);
}
