using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class IdentityResetCascade(
    IUserDeviceRepository devices,
    IUserDeviceListRepository lists,
    IDeviceLinkRequestRepository deviceLinks,
    ISemanticRelayLifecycleRepository semanticRelay,
    ISessionKeyBlobRepository blobs,
    ISessionRepository sessions,
    IIdentityResetAuditRepository audit,
    IUnitOfWork unitOfWork,
    LiveSessionTerminator liveSessionTerminator)
{
    private readonly IUserDeviceRepository _devices = devices;
    private readonly IUserDeviceListRepository _lists = lists;
    private readonly IDeviceLinkRequestRepository _deviceLinks = deviceLinks;
    private readonly ISemanticRelayLifecycleRepository _semanticRelay = semanticRelay;
    private readonly ISessionKeyBlobRepository _blobs = blobs;
    private readonly ISessionRepository _sessions = sessions;
    private readonly IIdentityResetAuditRepository _audit = audit;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;
    private readonly LiveSessionTerminator _liveSessionTerminator = liveSessionTerminator;

    internal async Task<IdentityResetCascadePlan> PrepareAsync(
        UserId userId,
        CancellationToken ct)
    {
        var activeOwnedSessionIds =
            await _sessions.GetActiveOwnedSessionIdsAsync(userId, ct);
        var removedDeviceIds = (await _devices.GetByUserIdAsync(userId, ct))
            .Select(device => device.DeviceId)
            .Order(StringComparer.Ordinal)
            .ToList();
        var sessionsWithRevokedKeys =
            (await _blobs.GetSessionIdsForRecipientDevicesAsync(
                removedDeviceIds,
                ct))
            .OrderBy(sessionId => sessionId.Value)
            .ToList();

        return new IdentityResetCascadePlan(
            activeOwnedSessionIds,
            removedDeviceIds,
            sessionsWithRevokedKeys);
    }

    internal async Task<IReadOnlyList<IdentityResetSessionTarget>> ResolveSessionTargetsAsync(
        IReadOnlyCollection<SessionId> sessionIds,
        CancellationToken ct)
    {
        if (sessionIds.Count == 0)
        {
            return [];
        }
        var currentSessions = await _sessions.GetByIdsForUpdateAsync(
            sessionIds
                .Distinct()
                .OrderBy(sessionId => sessionId.Value)
                .ToList(),
            ct);
        return currentSessions
            .Select(session => new IdentityResetSessionTarget(
                session.Id,
                session.IncarnationId))
            .OrderBy(target => target.SessionId.Value)
            .ThenBy(target => target.IncarnationId)
            .ToList();
    }


    public async Task<IdentityResetCascadeOutcome> ExecuteInsideExistingTransactionAsync(
        Guid resetId,
        UserId userId,
        IdentityResetAuditContext auditContext,
        IdentityResetCascadePlan plan,
        IReadOnlyCollection<IdentityResetSessionTarget> sessionTargets)
    {
        int devicesRemoved;
        int listsRemoved;

        var noCancellation = CancellationToken.None;
        var affectedSessionIds = plan.ActiveOwnedSessionIds
            .Concat(plan.SessionsWithRevokedKeys)
            .Distinct()
            .OrderBy(sessionId => sessionId.Value)
            .ToList();
        var lockedSessions = await _sessions.GetByIdsForUpdateAsync(
            affectedSessionIds,
            noCancellation);
        var sessionsStillHoldingRevokedKeys =
            (await _blobs.GetSessionIdsForRecipientDevicesAsync(
                plan.RemovedDeviceIds,
                noCancellation))
            .Distinct()
            .OrderBy(sessionId => sessionId.Value)
            .ToList();
        var sessionsStillHoldingRevokedKeysSet =
            sessionsStillHoldingRevokedKeys.ToHashSet();
        var lockedById = lockedSessions.ToDictionary(session => session.Id);
        var activeOwnedSessionIds = plan.ActiveOwnedSessionIds.ToHashSet();

        foreach (var session in lockedSessions)
        {
            if (activeOwnedSessionIds.Contains(session.Id)
                || session.IsOwner(userId)
                || session.Status == SessionStatus.Ended)
            {
                continue;
            }
            if (!sessionsStillHoldingRevokedKeysSet.Contains(session.Id))
            {
                continue;
            }

            session.FenceKeyPublication();
            await _sessions.UpdateAsync(session, noCancellation);
        }

        var sessionEnds =
            new List<LiveSessionTransitionResult>(plan.ActiveOwnedSessionIds.Count);
        foreach (var sessionId in plan.ActiveOwnedSessionIds)
        {
            if (!lockedById.TryGetValue(sessionId, out var session))
            {
                sessionEnds.Add(
                    new LiveSessionTransitionResult(
                        SessionTransitionOutcome.NotFound,
                        null));
                continue;
            }

            sessionEnds.Add(await _liveSessionTerminator
                .EndSessionInsideExistingTransactionAsync(
                    session,
                    noCancellation));
        }

        await _blobs.DeleteForRecipientDevicesAsync(
            plan.RemovedDeviceIds,
            noCancellation);

        await _deviceLinks.InvalidateOutstandingForUserAsync(
            userId,
            DateTimeOffset.UtcNow,
            noCancellation);
        await _semanticRelay.DeleteForAccountAsync(userId, noCancellation);
        devicesRemoved = await _devices.RemoveAllForUserAsync(userId, noCancellation);
        listsRemoved = await _lists.RemoveAllForUserAsync(userId, noCancellation);

        var endedSessionIds = sessionEnds
            .Where(end => end.Outcome.SatisfiesTargetState())
            .Select(end => end.SharingState!.SessionId)
            .ToList();

        await _audit.AddAsync(
            IdentityResetAuditEntry.Create(
                resetId,
                userId,
                sessionsAttempted: sessionEnds.Count,
                sessionsEnded: endedSessionIds.Count,
                devicesRemoved: devicesRemoved,
                deviceListsRemoved: listsRemoved,
                clientIp: auditContext.ClientIp,
                userAgent: auditContext.UserAgent,
                removedDeviceIds: plan.RemovedDeviceIds,
                endedSessionIds: endedSessionIds,
                endedSessionTargets: sessionEnds
                    .Where(end => end.Outcome.SatisfiesTargetState()
                        && end.SharingState is not null
                        && end.StartedAt.HasValue)
                    .Select(end => new IdentityResetEndedSessionTarget(
                        end.SharingState!.SessionId,
                        end.SharingState.IncarnationId,
                        end.SharingState.OwnerUserId,
                        end.SharingState.Scope,
                        end.SharingState.RoomId,
                        end.StartedAt!.Value))
                    .ToList(),
                sessionsWithRevokedKeys: sessionsStillHoldingRevokedKeys,
                sessionTargets: sessionTargets,
                audienceUserIds: plan.AudienceUserIds,
                identityRevision: plan.IdentityRevision),
            noCancellation);

        await _unitOfWork.SaveChangesAsync(noCancellation);

        return new IdentityResetCascadeOutcome(
            sessionEnds,
            plan.RemovedDeviceIds,
            sessionsStillHoldingRevokedKeys,
            devicesRemoved,
            listsRemoved);
    }
}

public sealed record IdentityResetCascadePlan(
    IReadOnlyList<SessionId> ActiveOwnedSessionIds,
    IReadOnlyList<string> RemovedDeviceIds,
    IReadOnlyList<SessionId> SessionsWithRevokedKeys)
{
    public IReadOnlyList<UserId> AudienceUserIds { get; init; } = [];
    public long IdentityRevision { get; init; }
}

public sealed record IdentityResetCascadeOutcome(
    IReadOnlyList<LiveSessionTransitionResult> SessionEnds,
        IReadOnlyList<string> RemovedDeviceIds,
    IReadOnlyList<SessionId> SessionsWithRevokedKeys,
    int DevicesRemoved,
    int ListsRemoved)
{
    public int SessionsAttempted => SessionEnds.Count;

    public IReadOnlyList<SessionId> EndedSessionIds =>
        SessionEnds
            .Where(end => end.Outcome.SatisfiesTargetState())
            .Select(end => end.SharingState!.SessionId)
            .ToList();
}
