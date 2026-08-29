using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SharingService(
    ISessionRepository sessions,
    IUserRepository users,
    IAccessOverrideRepository overrides,
    IAccessOverrideAuditRepository audit,
    ISessionAccessMutationRepository mutations,
    IUserLifecycleLock userLifecycleLock,
    ISessionEndAuthority sessionEndAuthority,
    IUnitOfWork unitOfWork,
    TimeProvider? timeProvider = null)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly IUserRepository _users = users;
    private readonly IAccessOverrideRepository _overrides = overrides;
    private readonly IAccessOverrideAuditRepository _audit = audit;
    private readonly ISessionAccessMutationRepository _mutations = mutations;
    private readonly IUserLifecycleLock _userLifecycleLock = userLifecycleLock;
    private readonly ISessionEndAuthority _sessionEndAuthority = sessionEndAuthority;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    public async Task<SharingMutationResult> GrantAccessAsync(
        SessionId sessionId,
        Guid expectedIncarnationId,
        Guid mutationId,
        UserId actorUserId,
        AccessLevel accessLevel,
        UserId grantedByUserId,
        DateTimeOffset expiresAt,
        CancellationToken ct = default,
        string? clientIp = null,
        string? userAgent = null)
    {
        var requestedExpiresAt = DateTimeOffset.FromUnixTimeMilliseconds(
            expiresAt.ToUnixTimeMilliseconds());
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _mutations.AcquireAsync(grantedByUserId, mutationId, ct);
        await _userLifecycleLock.AcquireAsync(grantedByUserId, ct);
        var lifecycle = await _sessionEndAuthority.AcquireAsync(
            [sessionId],
            ct);
        try
        {
            var session = await _sessions.GetByIdForUpdateAsync(sessionId, ct)
                ?? throw new NotFoundException(nameof(Session), sessionId);
            var existingMutation = await _mutations.GetAsync(
                grantedByUserId,
                mutationId,
                ct);
            if (existingMutation is not null)
            {
                if (!existingMutation.Matches(
                        sessionId,
                        expectedIncarnationId,
                        SessionAccessMutationKind.Grant,
                        actorUserId,
                        accessLevel,
                        requestedExpiresAt))
                {
                    throw new SessionAccessMutationTargetConflictException();
                }
                await transaction.CommitAsync(ct);
                return new SharingMutationResult(
                    sessionId,
                    expectedIncarnationId,
                    session.StartedAt,
                    session.OwnerUserId,
                    actorUserId,
                    Granted: true,
                    lifecycle);
            }

            if (!session.IsOwner(grantedByUserId)
                || session.Status == SessionStatus.Ended
                || session.IncarnationId != expectedIncarnationId)
            {
                throw new NotFoundException(nameof(Session), sessionId);
            }
            _ = await _users.GetByIdAsync(actorUserId, ct)
                ?? throw new NotFoundException("User", actorUserId);

            var now = _timeProvider.GetUtcNow();
            var resolvedExpiry = requestedExpiresAt;
            if (resolvedExpiry <= now || resolvedExpiry > now + SessionAccessOverride.MaximumLifetime)
            {
                throw new DomainException(
                    "Access expiry must be in the future and no more than 7 days away.");
            }

            var existing = await _overrides.GetAsync(sessionId, actorUserId, ct);
            if (existing is not null)
            {
                existing.Grant(accessLevel, grantedByUserId, resolvedExpiry, now);
                await _overrides.UpdateAsync(existing, ct);
            }
            else
            {
                var accessOverride = SessionAccessOverride.Create(
                    sessionId,
                    actorUserId,
                    accessLevel,
                    grantedByUserId,
                    resolvedExpiry,
                    now);

                await _overrides.AddAsync(accessOverride, ct);
            }

            await _mutations.AddAsync(
                SessionAccessMutation.Create(
                    grantedByUserId,
                    mutationId,
                    sessionId,
                    expectedIncarnationId,
                    SessionAccessMutationKind.Grant,
                    actorUserId,
                    accessLevel,
                    requestedExpiresAt,
                    now),
                ct);
            await _audit.AddAsync(
                AccessOverrideAuditEntry.Create(
                    sessionId,
                    grantedByUserId,
                    actorUserId,
                    AccessOverrideAuditAction.Granted,
                    AccessOverrideAuditReason.Explicit,
                    clientIp,
                    userAgent),
                ct);
            await _unitOfWork.SaveChangesAsync(ct);
            await transaction.CommitAsync(ct);
            return new SharingMutationResult(
                sessionId,
                expectedIncarnationId,
                session.StartedAt,
                session.OwnerUserId,
                actorUserId,
                Granted: true,
                lifecycle);
        }
        catch
        {
            await lifecycle.DisposeAsync();
            throw;
        }
    }

    public async Task<SharingMutationResult> RevokeAccessAsync(
        SessionId sessionId,
        Guid expectedIncarnationId,
        Guid mutationId,
        UserId actorUserId,
        UserId revokedByUserId,
        CancellationToken ct = default,
        string? clientIp = null,
        string? userAgent = null)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _mutations.AcquireAsync(revokedByUserId, mutationId, ct);
        await _userLifecycleLock.AcquireAsync(revokedByUserId, ct);
        var lifecycle = await _sessionEndAuthority.AcquireAsync(
            [sessionId],
            ct);
        try
        {
            var session = await _sessions.GetByIdForUpdateAsync(sessionId, ct)
                ?? throw new NotFoundException(nameof(Session), sessionId);
            var existingMutation = await _mutations.GetAsync(
                revokedByUserId,
                mutationId,
                ct);
            if (existingMutation is not null)
            {
                if (!existingMutation.Matches(
                        sessionId,
                        expectedIncarnationId,
                        SessionAccessMutationKind.Revoke,
                        actorUserId,
                        null,
                        null))
                {
                    throw new SessionAccessMutationTargetConflictException();
                }
                await transaction.CommitAsync(ct);
                return new SharingMutationResult(
                    sessionId,
                    expectedIncarnationId,
                    session.StartedAt,
                    session.OwnerUserId,
                    actorUserId,
                    Granted: false,
                    lifecycle);
            }

            if (!session.IsOwner(revokedByUserId)
                || session.Status == SessionStatus.Ended
                || session.IncarnationId != expectedIncarnationId)
            {
                throw new NotFoundException(nameof(Session), sessionId);
            }

            var existing = await _overrides.GetActiveAsync(sessionId, actorUserId, ct);
            if (existing is null)
            {
                throw new NotFoundException("AccessOverride", $"{sessionId}/{actorUserId}");
            }

            existing.Revoke();
            await _overrides.UpdateAsync(existing, ct);
            await _mutations.AddAsync(
                SessionAccessMutation.Create(
                    revokedByUserId,
                    mutationId,
                    sessionId,
                    expectedIncarnationId,
                    SessionAccessMutationKind.Revoke,
                    actorUserId,
                    null,
                    null,
                    _timeProvider.GetUtcNow()),
                ct);
            await _audit.AddAsync(
            AccessOverrideAuditEntry.Create(
                sessionId,
                revokedByUserId,
                actorUserId,
                AccessOverrideAuditAction.Revoked,
                AccessOverrideAuditReason.Explicit,
                clientIp,
                userAgent),
            ct);
            await _unitOfWork.SaveChangesAsync(ct);
            await transaction.CommitAsync(ct);
            return new SharingMutationResult(
                sessionId,
                expectedIncarnationId,
                session.StartedAt,
                session.OwnerUserId,
                actorUserId,
                Granted: false,
                lifecycle);
        }
        catch
        {
            await lifecycle.DisposeAsync();
            throw;
        }
    }
}

public sealed record SharingMutationResult(
    SessionId SessionId,
    Guid IncarnationId,
    DateTimeOffset StartedAt,
    UserId OwnerUserId,
    UserId ActorUserId,
    bool Granted,
    IAsyncDisposable? Lifecycle = null) : IAsyncDisposable
{
    public ValueTask DisposeAsync() =>
        Lifecycle?.DisposeAsync() ?? ValueTask.CompletedTask;
}
