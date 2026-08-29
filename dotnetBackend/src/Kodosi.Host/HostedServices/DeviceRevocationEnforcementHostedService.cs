using Kodosi.Application;

namespace Kodosi.Host.Realtime;

internal sealed class DeviceRevocationEnforcementHostedService(
    IDeviceRevocationDurabilityCoordinator durability,
    IDeviceListRealtimeEffects realtimeEffects,
    ILogger<DeviceRevocationEnforcementHostedService> logger,
    TimeProvider timeProvider) : BackgroundService
{
    private static readonly TimeSpan SweepInterval = TimeSpan.FromSeconds(5);
    private static readonly TimeSpan EnforcementTimeout = TimeSpan.FromSeconds(15);
    private const int BatchSize = 100;
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
        IReadOnlyList<DeviceRevocationEnforcementWork> pending;
        try
        {
            pending = await durability.GetPendingEnforcementAsync(BatchSize, ct);
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception exception)
        {
            logger.LogError(exception, "Failed to load pending device-revocation enforcement");
            return;
        }

        foreach (var work in pending)
        {
            try
            {
                using var attempt = CancellationTokenSource.CreateLinkedTokenSource(ct);
                attempt.CancelAfter(EnforcementAttemptTimeout);
                var enforced = await durability.ExecuteIfCurrentAsync(
                    work,
                    async enforcementCt =>
                    {
                        await realtimeEffects.EnforceCommittedAsync(
                            work.UserId,
                            work.RevokedDeviceIds,
                            work.AffectedSessionTargets,
                            enforcementCt);
                    },
                    attempt.Token);
                if (!enforced)
                {
                    logger.LogInformation(
                        "Skipped obsolete device-revocation enforcement revocation={RevocationId} user={UserId} revision={IdentityRevision} generation={Generation}",
                        work.RevocationId,
                        work.UserId.Value,
                        work.IdentityRevision,
                        work.DeviceListGeneration);
                }
                await durability.CompleteEnforcementAsync(work.RevocationId, ct);
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
                throw;
            }
            catch (OperationCanceledException)
            {
                logger.LogWarning(
                    "Timed out pending device-revocation enforcement; it remains queued revocation={RevocationId} user={UserId}",
                    work.RevocationId,
                    work.UserId.Value);
            }
            catch (Exception exception)
            {
                logger.LogError(
                    exception,
                    "Pending device-revocation enforcement failed revocation={RevocationId} user={UserId}",
                    work.RevocationId,
                    work.UserId.Value);
            }
        }
    }
}
