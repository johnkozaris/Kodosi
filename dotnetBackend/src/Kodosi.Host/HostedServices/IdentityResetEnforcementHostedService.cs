using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class IdentityResetEnforcementHostedService(
    IIdentityResetDurabilityCoordinator durabilityCoordinator,
    IIdentityResetRealtimeEnforcer realtimeEnforcer,
    IIdentityResetSessionResolver sessionResolver,
    ISessionEndAuthority sessionEndAuthority,
    ILogger<IdentityResetEnforcementHostedService> logger,
    TimeProvider timeProvider) : BackgroundService
{
    private static readonly TimeSpan SweepInterval = TimeSpan.FromSeconds(5);
    private static readonly TimeSpan EnforcementTimeout = TimeSpan.FromSeconds(15);
    private const int BatchSize = 100;

    private readonly IIdentityResetDurabilityCoordinator _durabilityCoordinator =
        durabilityCoordinator;
    private readonly IIdentityResetRealtimeEnforcer _realtimeEnforcer =
        realtimeEnforcer;
    private readonly IIdentityResetSessionResolver _sessionResolver = sessionResolver;
    private readonly ISessionEndAuthority _sessionEndAuthority = sessionEndAuthority;
    private readonly ILogger<IdentityResetEnforcementHostedService> _logger = logger;
    private readonly TimeProvider _timeProvider = timeProvider;
    internal TimeSpan EnforcementAttemptTimeout { get; set; } = EnforcementTimeout;

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        while (!stoppingToken.IsCancellationRequested)
        {
            await EnforcePendingAsync(stoppingToken);
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

    internal async Task EnforcePendingAsync(CancellationToken ct)
    {
        IReadOnlyList<IdentityResetEnforcementWork> pending;
        try
        {
            pending = await _durabilityCoordinator.GetPendingEnforcementAsync(
                BatchSize,
                ct);
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception exception)
        {
            _logger.LogError(
                exception,
                "Failed to load pending identity-reset realtime enforcement");
            return;
        }

        foreach (var work in pending)
        {
            try
            {
                using var attempt = CancellationTokenSource.CreateLinkedTokenSource(ct);
                attempt.CancelAfter(EnforcementAttemptTimeout);
                var enforced = await _durabilityCoordinator.ExecuteIfCurrentAsync(
                    work,
                    async enforcementCt =>
                    {
                        using var connectionFence = _realtimeEnforcer.FenceNewConnections(
                            work.UserId,
                            work.RemovedDeviceIds);
                        var currentTargets = await _sessionResolver.GetCurrentTargetsAsync(
                            work.SessionTargets,
                            enforcementCt);
                        var affectedSessionIds = currentTargets
                            .Select(target => target.SessionId)
                            .Distinct()
                            .OrderBy(sessionId => sessionId.Value)
                            .ToList();
                        await using var sessionLifecycles =
                            await _sessionEndAuthority.AcquireAsync(
                                affectedSessionIds,
                                enforcementCt);
                        var currentAfterLease = await _sessionResolver.GetCurrentTargetsAsync(
                            currentTargets,
                            enforcementCt);
                        var currentAfterLeaseSet = currentAfterLease.ToHashSet();
                        foreach (var ended in work.EndedSessions.Where(ended =>
                                     currentAfterLeaseSet.Contains(
                                         new IdentityResetSessionTarget(
                                             ended.Target.SessionId,
                                             ended.Target.IncarnationId))))
                        {
                            await _sessionEndAuthority.RetireEndedIncarnationAsync(
                                ended.Target,
                                ended.StartedAt,
                                enforcementCt);
                        }
                        await _realtimeEnforcer.EnforceCommittedAsync(
                            work.UserId,
                            work.RemovedDeviceIds,
                            currentAfterLease
                                .Where(target => work.SessionsWithRevokedKeys.Contains(
                                    target.SessionId))
                                .Select(target => target.SessionId)
                                .ToList(),
                            enforcementCt);
                    },
                    attempt.Token);
                if (!enforced)
                {
                    _logger.LogInformation(
                        "Skipped obsolete identity-reset enforcement reset={ResetId} user={UserId} revision={IdentityRevision}",
                        work.ResetId,
                        work.UserId.Value,
                        work.IdentityRevision);
                }
                await _durabilityCoordinator.CompleteEnforcementAsync(
                    work.ResetId,
                    ct);
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
                throw;
            }
            catch (OperationCanceledException)
            {
                _logger.LogWarning(
                    "Timed out pending identity-reset realtime enforcement; it remains queued reset={ResetId} user={UserId}",
                    work.ResetId,
                    work.UserId.Value);
            }
            catch (Exception exception)
            {
                _logger.LogError(
                    exception,
                    "Pending identity-reset realtime enforcement failed reset={ResetId} user={UserId}",
                    work.ResetId,
                    work.UserId.Value);
            }
        }
    }
}
