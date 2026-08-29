using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Realtime;

namespace Kodosi.Host;

public sealed class CleanupHostedService(
    IActionDedupeCache dedupeCache,
    IFriendRequestThrottle friendRequestThrottle,
    IDeviceLinkPollThrottle deviceLinkPollThrottle,
    IServiceScopeFactory scopeFactory,
    ISessionEndAuthority sessionEndAuthority,
    ILogger<CleanupHostedService> logger,
    TimeProvider? timeProvider = null) : BackgroundService
{
    private static readonly TimeSpan SweepInterval = TimeSpan.FromMinutes(1);

    private static readonly TimeSpan DeviceLinkSweepInterval = TimeSpan.FromMinutes(5);
    private static readonly TimeSpan SemanticReceiptRetention = TimeSpan.FromHours(24);
    private static readonly TimeSpan ImmediateEnforcementTimeout = TimeSpan.FromSeconds(15);
    private const int SemanticReceiptSweepBatch = 1_000;

    private readonly IActionDedupeCache _dedupeCache = dedupeCache;
    private readonly IFriendRequestThrottle _friendRequestThrottle = friendRequestThrottle;
    private readonly IDeviceLinkPollThrottle _deviceLinkPollThrottle = deviceLinkPollThrottle;
    private readonly IServiceScopeFactory _scopeFactory = scopeFactory;
    private readonly ISessionEndAuthority _sessionEndAuthority = sessionEndAuthority;
    private readonly ILogger<CleanupHostedService> _logger = logger;
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;
    private DateTimeOffset _lastDeviceLinkSweep = DateTimeOffset.MinValue;
    internal TimeSpan ImmediateEnforcementAttemptTimeout { get; set; } =
        ImmediateEnforcementTimeout;
    internal TimeSpan LifecycleGateAttemptTimeout { get; set; } =
        ImmediateEnforcementTimeout;

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                _dedupeCache.Sweep();
                _friendRequestThrottle.Sweep();
                _deviceLinkPollThrottle.Sweep();

                var now = _timeProvider.GetUtcNow();
                await SweepExpiredAccessOverridesAsync(now, stoppingToken);
                await SweepStaleInvitationsAsync(now, stoppingToken);
                if (now - _lastDeviceLinkSweep >= DeviceLinkSweepInterval)
                {
                    await SweepDeviceLinkRequestsAsync(now, stoppingToken);
                    await SweepExpiredDeviceRegistrationChallengesAsync(now, stoppingToken);
                    await SweepAcknowledgedSemanticReceiptsAsync(now, stoppingToken);
                    _lastDeviceLinkSweep = now;
                }
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
            catch (Exception ex)
            {
                _logger.LogError(ex, "Error in cleanup sweep");
            }

            try
            {
                await Task.Delay(SweepInterval, _timeProvider, stoppingToken);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
        }
    }

    private async Task SweepDeviceLinkRequestsAsync(DateTimeOffset now, CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var repository = scope.ServiceProvider.GetRequiredService<IDeviceLinkRequestRepository>();
        var removed = await repository.DeleteStaleAsync(now, ct);
        if (removed > 0)
        {
            _logger.LogInformation(
                "Swept {Removed} stale device-link request rows", removed);
        }
    }

    internal async Task SweepAcknowledgedSemanticReceiptsAsync(
        DateTimeOffset now,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var repository = scope.ServiceProvider.GetRequiredService<ISemanticRelayRepository>();
        var removed = await repository.DeleteAcknowledgedBeforeAsync(
            now.Subtract(SemanticReceiptRetention),
            SemanticReceiptSweepBatch,
            ct);
        if (removed > 0)
        {
            _logger.LogInformation(
                "Swept {Removed} acknowledged semantic relay request rows",
                removed);
        }
    }

    private async Task SweepExpiredDeviceRegistrationChallengesAsync(
        DateTimeOffset now,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var repository = scope.ServiceProvider
            .GetRequiredService<IDeviceRegistrationChallengeRepository>();
        var removed = await repository.DeleteExpiredAsync(now, 1_000, ct);
        if (removed > 0)
        {
            _logger.LogInformation(
                "Swept {Removed} expired device-registration challenge rows",
                removed);
        }
    }

    internal async Task SweepExpiredAccessOverridesAsync(
        DateTimeOffset now,
        CancellationToken ct)
    {
        IReadOnlyList<SessionAccessOverride> expired;
        await using (var scope = _scopeFactory.CreateAsyncScope())
        {
            var overrides = scope.ServiceProvider.GetRequiredService<IAccessOverrideRepository>();
            expired = await overrides.GetExpiredUnrevokedAsync(now, 500, ct);
        }
        if (expired.Count == 0)
        {
            return;
        }

        var revokedCount = 0;
        foreach (var accessOverride in expired)
        {
            if (accessOverride.ExpiresAt is not { } observedExpiry)
            {
                continue;
            }

            using var attempt = CancellationTokenSource.CreateLinkedTokenSource(ct);
            attempt.CancelAfter(LifecycleGateAttemptTimeout);
            await using var sessionLifecycle = await _sessionEndAuthority.AcquireAsync(
                [accessOverride.SessionId],
                attempt.Token);
            AccessOverrideExpiryEnforcementWork work;
            UserId viewerId;
            SessionAccessFanoutTarget target;
            await using (var scope = _scopeFactory.CreateAsyncScope())
            {
                var overrides = scope.ServiceProvider
                    .GetRequiredService<IAccessOverrideRepository>();
                var audit = scope.ServiceProvider
                    .GetRequiredService<IAccessOverrideAuditRepository>();
                var sessions = scope.ServiceProvider.GetRequiredService<ISessionRepository>();
                var unitOfWork = scope.ServiceProvider.GetRequiredService<IUnitOfWork>();
                await using var transaction = await unitOfWork.BeginTransactionAsync(ct);
                var session = await sessions.GetByIdAsync(
                    accessOverride.SessionId,
                    ct);
                if (session is null
                    || !await overrides.TryRevokeExpiredAsync(
                        accessOverride.SessionId,
                        accessOverride.ActorUserId,
                        observedExpiry,
                        now,
                        ct))
                {
                    continue;
                }
                target = new SessionAccessFanoutTarget(
                    session.Id,
                    session.StartedAt,
                    session.IncarnationId);
                var auditEntry = AccessOverrideAuditEntry.CreateExpiredRevocation(
                    accessOverride.SessionId,
                    accessOverride.GrantedByUserId,
                    accessOverride.ActorUserId,
                    target.IncarnationId,
                    target.StartedAt,
                    observedExpiry,
                    now);
                work = new AccessOverrideExpiryEnforcementWork(
                    auditEntry.Id,
                    auditEntry.OccurredAt,
                    target.SessionId,
                    accessOverride.ActorUserId,
                    target.IncarnationId,
                    target.StartedAt,
                    observedExpiry,
                    now);
                viewerId = accessOverride.ActorUserId;
                await audit.AddAsync(auditEntry, ct);
                await unitOfWork.SaveChangesAsync(ct);
                await transaction.CommitAsync(ct);
                revokedCount++;
            }



            try
            {
                using var enforcementAttempt = CancellationTokenSource.CreateLinkedTokenSource(ct);
                enforcementAttempt.CancelAfter(ImmediateEnforcementAttemptTimeout);
                await using var enforcementScope = _scopeFactory.CreateAsyncScope();
                var disconnector = enforcementScope.ServiceProvider
                    .GetRequiredService<SessionAccessDisconnector>();
                var durability = enforcementScope.ServiceProvider
                    .GetRequiredService<IAccessOverrideExpiryDurabilityCoordinator>();
                _ = await durability.ExecuteIfCurrentAsync(
                    work,
                    enforcementCt => disconnector.DisconnectExpiredOverrideAsync(
                        target,
                        viewerId,
                        enforcementCt,
                        lifecycleAlreadyHeld: true),
                    enforcementAttempt.Token);
                await durability.CompleteEnforcementAsync(
                    work.AuditEntryId,
                    enforcementAttempt.Token);
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
                throw;
            }
            catch (OperationCanceledException)
            {
                _logger.LogWarning(
                    "Timed out immediate access-override expiry enforcement; durable work remains queued audit={AuditEntryId} session={SessionId} viewer={ViewerId}",
                    work.AuditEntryId,
                    target.SessionId.Value,
                    viewerId.Value);
            }
            catch (Exception exception)
            {
                _logger.LogError(
                    exception,
                    "Immediate access-override expiry enforcement failed; durable work remains queued audit={AuditEntryId} session={SessionId} viewer={ViewerId}",
                    work.AuditEntryId,
                    target.SessionId.Value,
                    viewerId.Value);
            }
        }

        _logger.LogInformation(
            "Expired {Count} session guest access overrides",
            revokedCount);
    }

    private async Task SweepStaleInvitationsAsync(
        DateTimeOffset now,
        CancellationToken ct)
    {
        await using var scope = _scopeFactory.CreateAsyncScope();
        var invitations = scope.ServiceProvider.GetRequiredService<IRoomInvitationRepository>();
        var unitOfWork = scope.ServiceProvider.GetRequiredService<IUnitOfWork>();
        var events = scope.ServiceProvider.GetRequiredService<SharedSurfaceEventPublisher>();
        var roomLifecycleLock = scope.ServiceProvider.GetRequiredService<IRoomLifecycleLock>();
        var rooms = scope.ServiceProvider.GetRequiredService<IRoomRepository>();
        await using var transaction = await unitOfWork.BeginTransactionAsync(ct);

        var candidates = (await invitations.GetExpiredPendingAsync(now, 500, ct))
            .Concat(await invitations.GetGenerationDriftPendingAsync(500, ct))
            .GroupBy(invitation => invitation.Id)
            .Select(group => group.First())
            .ToList();
        if (candidates.Count == 0)
        {
            return;
        }

        foreach (var roomId in candidates
            .Select(invitation => invitation.RoomId)
            .Distinct()
            .OrderBy(roomId => roomId.Value))
        {
            await roomLifecycleLock.AcquireAsync(roomId, ct);
        }

        var changed = new List<RoomInvitation>();
        var current = await invitations.GetByIdsAsync(
            candidates.Select(invitation => invitation.Id).ToList(),
            ct);
        foreach (var invitation in current)
        {
            if (invitation.Status != RoomInvitationStatus.Pending)
            {
                continue;
            }
            if (invitation.IsExpired(now))
            {
                invitation.Expire(now);
                changed.Add(invitation);
                continue;
            }
            var room = await rooms.GetByIdAsync(invitation.RoomId, ct);
            if (room is not null
                && room.RosterGeneration != invitation.BaseRosterGeneration)
            {
                invitation.Supersede(now);
                changed.Add(invitation);
            }
        }
        if (changed.Count == 0)
        {
            return;
        }

        await unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        foreach (var invitation in changed)
        {
            events.PublishRoomInvitationsChanged(
                invitation.InvitedByUserId,
                invitation.InviteeUserId);
        }

        _logger.LogInformation(
            "Closed {Count} expired or roster-stale room invitations",
            changed.Count);
    }
}
