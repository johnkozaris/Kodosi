using Kodosi.Domain;
using Microsoft.Extensions.Logging;

namespace Kodosi.Application;

public sealed class IdentityLifecycleService(
    IUserDeviceRepository devices,
    IUserDeviceListRepository deviceLists,
    IUserRepository users,
    IFriendshipRepository friendships,
    IRoomMemberRepository rooms,
    IAccessOverrideRepository accessOverrides,
    IIdentityExposureRepository identityExposures,
    IdentityResetPopVerifier popVerifier,
    IdentityResetCascade resetCascade,
    IUnitOfWork unitOfWork,
    IUserLifecycleLock userLifecycleLock,
    IRecipientDeviceLifecycleLock recipientDeviceLifecycleLock,
    ISessionEndAuthority sessionEndAuthority,
    IIdentityResetRealtimeEnforcer realtimeEnforcer,
    IIdentityResetDurabilityCoordinator durabilityCoordinator,
    ILogger<IdentityLifecycleService> logger)
{
    private readonly IUserDeviceRepository _devices = devices;
    private readonly IUserDeviceListRepository _deviceLists = deviceLists;
    private readonly IUserRepository _users = users;
    private readonly IFriendshipRepository _friendships = friendships;
    private readonly IRoomMemberRepository _rooms = rooms;
    private readonly IAccessOverrideRepository _accessOverrides = accessOverrides;
    private readonly IIdentityExposureRepository _identityExposures = identityExposures;
    private readonly IdentityResetPopVerifier _popVerifier = popVerifier;
    private readonly IdentityResetCascade _resetCascade = resetCascade;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;
    private readonly IUserLifecycleLock _userLifecycleLock = userLifecycleLock;
    private readonly IRecipientDeviceLifecycleLock _recipientDeviceLifecycleLock =
        recipientDeviceLifecycleLock;
    private readonly ISessionEndAuthority _sessionEndAuthority = sessionEndAuthority;
    private readonly IIdentityResetRealtimeEnforcer _realtimeEnforcer = realtimeEnforcer;
    private readonly IIdentityResetDurabilityCoordinator _durabilityCoordinator =
        durabilityCoordinator;
    private readonly ILogger<IdentityLifecycleService> _logger = logger;



    public async Task<IdentityResetResult> ResetIdentityAsync(
        UserId userId,
        IdentityResetPopPayload? popPayload,
        IdentityResetAuditContext auditContext,
        CancellationToken ct = default)
    {
        ITransactionScope? transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _recipientDeviceLifecycleLock.AcquireAsync([userId], ct);
        try
        {
            await _userLifecycleLock.AcquireAsync(userId, ct);



            var now = DateTimeOffset.UtcNow;
            var currentList = await _deviceLists.GetLatestAsync(userId, ct);
            var activeEnrolled = (await _devices.GetByUserIdAsync(userId, ct))
                .Where(device => ActiveDeviceAuthorization.IsAuthorized(
                    device,
                    currentList,
                    userId,
                    now))
                .ToList();

            if (activeEnrolled.Count > 0)
            {
                try
                {
                    await _popVerifier.VerifyAsync(userId, popPayload, activeEnrolled, ct);
                }
                catch (DeviceEnrollmentException)
                {


                    await transaction.CommitAsync(CancellationToken.None);
                    throw;
                }
            }

            var user = await _users.GetByIdAsync(userId, CancellationToken.None)
                ?? throw new NotFoundException("User", userId.Value);
            var identityRevision = user.AdvanceIdentityLifecycle(incarnationId: null);
            var audienceUserIds = await ResolveResetAudienceAsync(
                userId,
                CancellationToken.None);
            var plan = (await _resetCascade.PrepareAsync(
                userId,
                CancellationToken.None)) with
            {
                AudienceUserIds = audienceUserIds,
                IdentityRevision = identityRevision,
            };
            using var realtimeFence = _realtimeEnforcer.FenceNewConnections(
                userId,
                plan.RemovedDeviceIds);
            var affectedSessionIds = plan.ActiveOwnedSessionIds
                .Concat(_realtimeEnforcer.GetAffectedSessionIds(
                    userId,
                    plan.RemovedDeviceIds,
                    plan.SessionsWithRevokedKeys))
                .Distinct()
                .OrderBy(sessionId => sessionId.Value)
                .ToList();


            await using var sessionLifecycles = await _sessionEndAuthority.AcquireAsync(
                affectedSessionIds,
                CancellationToken.None);
            var sessionTargets = await _resetCascade.ResolveSessionTargetsAsync(
                affectedSessionIds,
                CancellationToken.None);
            var resetId = Guid.NewGuid();
            var cascadeOutcome =
                await _resetCascade.ExecuteInsideExistingTransactionAsync(
                    resetId,
                    userId,
                    auditContext,
                    plan,
                    sessionTargets);

            try
            {
                await transaction.CommitAsync(CancellationToken.None);
            }
            catch (Exception commitException)
            {
                try
                {
                    await transaction.DisposeAsync();
                }
                catch (Exception disposeException)
                {
                    _logger.LogWarning(
                        disposeException,
                        "identity_reset failed transaction disposal before commit reconciliation reset={ResetId} user={UserId}",
                        resetId,
                        userId.Value);
                }
                transaction = null;

                IdentityResetCommitReconciliation reconciliation;
                try
                {
                    reconciliation =
                        await _durabilityCoordinator.ReconcileCommitAsync(
                            resetId,
                            userId,
                            plan.RemovedDeviceIds,
                            CancellationToken.None);
                }
                catch (Exception reconciliationException)
                {
                    throw new InvalidOperationException(
                        "Identity reset commit outcome could not be reconciled.",
                        new AggregateException(
                            commitException,
                            reconciliationException));
                }

                if (reconciliation.State == IdentityResetCommitState.NotCommitted)
                {
                    throw;
                }
                if (reconciliation.State != IdentityResetCommitState.Committed)
                {
                    throw new InvalidOperationException(
                        "Identity reset commit reconciliation found inconsistent durable state: "
                        + reconciliation.Detail,
                        commitException);
                }

                _logger.LogWarning(
                    commitException,
                    "identity_reset commit reported failure but durable reset was committed reset={ResetId} user={UserId}",
                    resetId,
                    userId.Value);
            }

            await _sessionEndAuthority.ProjectCommittedAsync(
                cascadeOutcome.SessionEnds
                    .Select(end => new CommittedSessionEnd(
                        end,
                        CommittedSessionEndReason.OwnerIdentityReset))
                    .ToList(),
                CancellationToken.None);
            await _realtimeEnforcer.EnforceCommittedAsync(
                userId,
                cascadeOutcome.RemovedDeviceIds,
                cascadeOutcome.SessionsWithRevokedKeys,
                CancellationToken.None);

            try
            {
                await _durabilityCoordinator.CompleteEnforcementAsync(
                    resetId,
                    CancellationToken.None);
            }
            catch (Exception completionException)
            {
                _logger.LogWarning(
                    completionException,
                    "identity_reset durable enforcement marker remains pending reset={ResetId} user={UserId}",
                    resetId,
                    userId.Value);
            }

            return new IdentityResetResult(
                EndedSessionIds: cascadeOutcome.EndedSessionIds,
                SessionsAttempted: cascadeOutcome.SessionsAttempted,
                AudienceUserIds: plan.AudienceUserIds,
                IdentityRevision: plan.IdentityRevision,
                DevicesRemoved: cascadeOutcome.DevicesRemoved,
                ListsRemoved: cascadeOutcome.ListsRemoved);
        }
        finally
        {
            if (transaction is not null)
            {
                await transaction.DisposeAsync();
            }
        }
    }

    private async Task<IReadOnlyList<UserId>> ResolveResetAudienceAsync(
        UserId userId,
        CancellationToken ct = default)
    {
        var friendIds = await _friendships.GetFriendIdsAsync(userId, ct);
        var roomPeerIds = await _rooms.GetRoomPeerUserIdsAsync(userId, ct);
        var overridePeerIds = await _accessOverrides.GetActiveRelatedUserIdsAsync(userId, ct);
        var historicalPeerIds = await _identityExposures.GetHistoricalPeerUserIdsAsync(userId, ct);
        return DeviceListAudience.Build(
            friendIds.Concat(overridePeerIds).Concat(historicalPeerIds).ToList(),
            roomPeerIds,
            userId);
    }
}

public sealed record IdentityResetPopPayload(
    Guid ChallengeId,
    string SignerDeviceId,
    string PopSignature);

public sealed record IdentityResetAuditContext(
    string? ClientIp,
    string? UserAgent);

public sealed record IdentityResetResult(
    IReadOnlyList<SessionId> EndedSessionIds,
    int SessionsAttempted,
    IReadOnlyList<UserId> AudienceUserIds,
    long IdentityRevision,
    int DevicesRemoved,
    int ListsRemoved);
