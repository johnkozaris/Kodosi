using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class RoomMembershipWorkflowService(
    RoomService rooms,
    SessionAccessOverrideRevoker accessRevoker,
    IRoomMemberAuditRepository audit,
    IUnitOfWork unitOfWork,
    IRoomLifecycleLock roomLifecycleLock,
    ISessionEndAuthority sessionEndAuthority,
    IRoomMutationReceiptRepository mutationReceipts,
    TimeProvider? timeProvider = null)
{
    private readonly RoomService _rooms = rooms;
    private readonly SessionAccessOverrideRevoker _accessRevoker = accessRevoker;
    private readonly IRoomMemberAuditRepository _audit = audit;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;
    private readonly IRoomLifecycleLock _roomLifecycleLock = roomLifecycleLock;
    private readonly ISessionEndAuthority _sessionEndAuthority = sessionEndAuthority;
    private readonly IRoomMutationReceiptRepository _mutationReceipts = mutationReceipts;
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    public async Task<RoomMemberRemovalResult> RemoveMemberIdempotentlyAsync(
        Guid requestId,
        RoomId roomId,
        UserId actorUserId,
        UserId removedUserId,
        long rosterGeneration,
        byte[] rosterBody,
        byte[] rosterSignature,
        string rosterSignerDeviceId,
        RequestAuditContext auditContext,
        CancellationToken ct = default)
    {
        await using var tx = await _unitOfWork.BeginTransactionAsync(ct);
        await _mutationReceipts.AcquireAsync(
            actorUserId,
            RoomMutationOperation.RemoveMember,
            requestId,
            ct);
        _ = await _rooms.GetByIdAsync(roomId, ct)
            ?? throw new NotFoundException(nameof(Room), roomId);
        var fingerprint = RoomMutationTargetFingerprint.RemoveMember(
            roomId,
            removedUserId,
            checked(rosterGeneration - 1),
            rosterGeneration,
            rosterBody,
            rosterSignature,
            rosterSignerDeviceId);
        var existingReceipt = await _mutationReceipts.GetAsync(
            actorUserId,
            RoomMutationOperation.RemoveMember,
            requestId,
            ct);
        RoomMutationReceiptResult? duplicateReceipt = null;
        if (existingReceipt is not null)
        {
            RoomMutationReceiptPolicy.EnsureOperation(
                existingReceipt,
                RoomMutationOperation.RemoveMember);
            duplicateReceipt = RoomMutationReceiptPolicy.ResolveDuplicate(
                existingReceipt,
                fingerprint);
        }

        await _roomLifecycleLock.AcquireAsync(roomId, ct);
        IReadOnlyList<RoomMutationSessionEffect>? storedEffects = null;
        IReadOnlyList<SessionId> candidateSessionIds;
        if (duplicateReceipt is not null)
        {
            storedEffects = await _mutationReceipts.GetSessionEffectsAsync(
                actorUserId,
                requestId,
                ct);
            candidateSessionIds = storedEffects
                .Select(effect => effect.SessionId)
                .Distinct()
                .ToList();
        }
        else
        {
            candidateSessionIds = await _accessRevoker.GetNonEndedRoomSessionIdsAsync(
                roomId,
                ct);
        }
        var sessionLifecycles = await _sessionEndAuthority.AcquireAsync(
            candidateSessionIds,
            ct);
        try
        {
            var noCancellation = CancellationToken.None;
            if (duplicateReceipt is not null)
            {
                var replaySessions = storedEffects!
                    .Select(effect => ToRoomLiveSession(effect, existingReceipt!.RoomId))
                    .ToList();
                await tx.CommitAsync(noCancellation);
                await ProjectCommittedSessionEndsAsync(replaySessions, noCancellation);
                return new RoomMemberRemovalResult(
                    removedUserId,
                    replaySessions,
                    sessionLifecycles,
                    IsDuplicate: true,
                    Receipt: duplicateReceipt);
            }

            await _rooms.RemoveMemberWithLifecycleHeldAsync(
                roomId,
                actorUserId,
                removedUserId,
                rosterGeneration,
                rosterBody,
                rosterSignature,
                rosterSignerDeviceId,
                noCancellation);
            var affectedSessions = await _accessRevoker.RevokeRoomMemberOverridesAsync(
                roomId, removedUserId, candidateSessionIds, noCancellation);
            await AddAuditAsync(
                roomId,
                actorUserId,
                removedUserId,
                RoomMemberAuditAction.Removed,
                auditContext,
                noCancellation);
            var receipt = RoomMutationReceiptPolicy.Create(
                actorUserId,
                RoomMutationOperation.RemoveMember,
                requestId,
                fingerprint,
                roomId,
                removedUserId.Value,
                "Removed",
                rosterGeneration,
                assigneeSessionId: null,
                assigneeSessionIncarnationId: null,
                _timeProvider.GetUtcNow());
            await _mutationReceipts.AddAsync(receipt, noCancellation);
            await _mutationReceipts.AddSessionEffectsAsync(
                affectedSessions.Select(session => RoomMutationSessionEffect.Create(
                    actorUserId,
                    requestId,
                    session.SessionId,
                    session.IncarnationId,
                    session.OwnerId,
                    session.StartedAt,
                    session.EndedByRemoval)).ToList(),
                noCancellation);
            await _unitOfWork.SaveChangesAsync(noCancellation);
            await tx.CommitAsync(noCancellation);
            await ProjectCommittedSessionEndsAsync(affectedSessions, noCancellation);

            return new RoomMemberRemovalResult(
                removedUserId,
                affectedSessions,
                sessionLifecycles,
                IsDuplicate: false,
                Receipt: new RoomMutationReceiptResult(
                    false,
                    receipt.RoomId,
                    receipt.EntityId,
                    receipt.Result,
                    receipt.Revision,
                    null,
                    null));
        }
        catch
        {
            await sessionLifecycles.DisposeAsync();
            throw;
        }
    }

    private static RoomLiveSession ToRoomLiveSession(
        RoomMutationSessionEffect effect,
        RoomId receiptRoomId)
    {
        var sharingState = new SessionDiscoveryTarget(
            effect.SessionId,
            effect.OwnerUserId,
            SessionScope.Room,
            receiptRoomId,
            effect.SessionIncarnationId);
        return new RoomLiveSession(
            effect.SessionId,
            effect.OwnerUserId,
            effect.StartedAt,
            effect.SessionIncarnationId,
            effect.EndedByRemoval
                ? new LiveSessionTransitionResult(
                    SessionTransitionOutcome.Applied,
                    sharingState,
                    effect.StartedAt)
                : null);
    }

    private Task ProjectCommittedSessionEndsAsync(
        IReadOnlyList<RoomLiveSession> sessions,
        CancellationToken ct) =>
        _sessionEndAuthority.ProjectCommittedAsync(
            sessions
                .Where(session => session.EndTransition is not null)
                .Select(session => new CommittedSessionEnd(
                    session.EndTransition!,
                    CommittedSessionEndReason.AccessRevoked))
                .ToList(),
            ct);

    private Task AddAuditAsync(
        RoomId roomId,
        UserId actorUserId,
        UserId targetUserId,
        RoomMemberAuditAction action,
        RequestAuditContext auditContext,
        CancellationToken ct)
        => _audit.AddAsync(
            RoomMemberAuditEntry.Create(
                roomId,
                actorUserId,
                targetUserId,
                action,
                auditContext.ClientIp,
                auditContext.UserAgent),
            ct);
}

public sealed record RoomMemberRemovalResult(
    UserId RemovedUserId,
    IReadOnlyList<RoomLiveSession> AffectedSessions,
    IAsyncDisposable? Lifecycle = null,
    bool IsDuplicate = false,
    RoomMutationReceiptResult? Receipt = null) : IAsyncDisposable
{
    public ValueTask DisposeAsync() =>
        Lifecycle?.DisposeAsync() ?? ValueTask.CompletedTask;
}
