using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Realtime;

internal sealed class AccessOverrideExpiryEnforcementHostedService(
    IAccessOverrideExpiryDurabilityCoordinator durability,
    IServiceScopeFactory scopeFactory,
    ISessionEndAuthority sessionEndAuthority,
    ILogger<AccessOverrideExpiryEnforcementHostedService> logger,
    TimeProvider timeProvider) : BackgroundService
{
    private static readonly TimeSpan SweepInterval = TimeSpan.FromSeconds(5);
    private static readonly TimeSpan EnforcementTimeout = TimeSpan.FromSeconds(15);
    private const int BatchSize = 100;
    private AccessOverrideExpiryEnforcementCursor? _cursor;
    internal TimeSpan EnforcementAttemptTimeout { get; set; } = EnforcementTimeout;

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        while (!stoppingToken.IsCancellationRequested)
        {
            await EnforcePendingAsync(stoppingToken);
            try
            {
                await Task.Delay(SweepInterval, timeProvider, stoppingToken);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
        }
    }

    internal async Task EnforcePendingAsync(CancellationToken ct)
    {
        IReadOnlyList<AccessOverrideExpiryEnforcementWork> pending;
        try
        {
            pending = await durability.GetPendingEnforcementAsync(BatchSize, ct, _cursor);
            if (pending.Count == 0 && _cursor is not null)
            {
                _cursor = null;
                pending = await durability.GetPendingEnforcementAsync(BatchSize, ct);
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception exception)
        {
            logger.LogError(
                exception,
                "Failed to load pending access-override expiry enforcement");
            return;
        }
        if (pending.Count == 0)
        {
            return;
        }

        foreach (var work in pending)
        {
            try
            {
                using var attempt = CancellationTokenSource.CreateLinkedTokenSource(ct);
                attempt.CancelAfter(EnforcementAttemptTimeout);
                await using var sessionLifecycle = await sessionEndAuthority.AcquireAsync(
                    [work.SessionId],
                    attempt.Token);
                var enforced = await durability.ExecuteIfCurrentAsync(
                    work,
                    async enforcementCt =>
                    {
                        await using var scope = scopeFactory.CreateAsyncScope();
                        var disconnector = scope.ServiceProvider
                            .GetRequiredService<SessionAccessDisconnector>();
                        await disconnector.DisconnectExpiredOverrideAsync(
                            work.Target,
                            work.GranteeUserId,
                            enforcementCt,
                            lifecycleAlreadyHeld: true);
                    },
                    attempt.Token);
                if (!enforced)
                {
                    logger.LogInformation(
                        "Skipped obsolete access-override expiry enforcement audit={AuditEntryId} session={SessionId} viewer={ViewerId}",
                        work.AuditEntryId,
                        work.SessionId.Value,
                        work.GranteeUserId.Value);
                }
                await durability.CompleteEnforcementAsync(
                    work.AuditEntryId,
                    CancellationToken.None);
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
                throw;
            }
            catch (OperationCanceledException)
            {
                logger.LogWarning(
                    "Timed out pending access-override expiry enforcement; it remains queued audit={AuditEntryId} session={SessionId} viewer={ViewerId}",
                    work.AuditEntryId,
                    work.SessionId.Value,
                    work.GranteeUserId.Value);
            }
            catch (Exception exception)
            {
                logger.LogError(
                    exception,
                    "Pending access-override expiry enforcement failed audit={AuditEntryId} session={SessionId} viewer={ViewerId}",
                    work.AuditEntryId,
                    work.SessionId.Value,
                    work.GranteeUserId.Value);
            }
        }

        var oldest = pending[^1];
        _cursor = new AccessOverrideExpiryEnforcementCursor(
            oldest.OccurredAt,
            oldest.AuditEntryId);
    }
}
