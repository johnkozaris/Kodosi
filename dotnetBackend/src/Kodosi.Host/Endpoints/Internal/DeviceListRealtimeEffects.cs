using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Realtime;

namespace Kodosi.Host.Endpoints;

internal sealed class DeviceListRealtimeEffects : IDeviceListRealtimeEffects
{
    private readonly RealtimeDeviceAccessEnforcementCore _enforcement;
    private readonly UserEventBroadcaster _userEvents;
    private readonly ISessionEndAuthority _sessionEndAuthority;
    private readonly IDeviceRevocationSessionResolver _sessionResolver;
    private readonly ILiveSessionStateDirectory _runtimes;
    private readonly ILogger<DeviceListRealtimeEffects> _logger;

    public DeviceListRealtimeEffects(
        RealtimeDeviceAccessEnforcementCore enforcement,
        UserEventBroadcaster userEvents,
        ISessionEndAuthority sessionEndAuthority,
        IDeviceRevocationSessionResolver sessionResolver,
        ILiveSessionStateDirectory runtimes,
        ILogger<DeviceListRealtimeEffects> logger)
    {
        _enforcement = enforcement;
        _userEvents = userEvents;
        _sessionEndAuthority = sessionEndAuthority;
        _sessionResolver = sessionResolver;
        _runtimes = runtimes;
        _logger = logger;
    }

    public Task PersistAndEnforceAsync(
        UserId userId,
        IReadOnlyCollection<string> revokedDeviceIds,
        IReadOnlyCollection<DeviceRevocationSessionTarget> sessionsWithRevokedKeys,
        Func<CancellationToken, Task> persistAndCommitAsync,
        CancellationToken ct = default) =>
        EnforceAsync(
            userId,
            revokedDeviceIds,
            sessionsWithRevokedKeys,
            persistAndCommitAsync,
            ct);

    public Task EnforceCommittedAsync(
        UserId userId,
        IReadOnlyCollection<string> revokedDeviceIds,
        IReadOnlyCollection<DeviceRevocationSessionTarget> sessionsWithRevokedKeys,
        CancellationToken ct = default) =>
        EnforceAsync(
            userId,
            revokedDeviceIds,
            sessionsWithRevokedKeys,
            null,
            ct);

    private async Task EnforceAsync(
        UserId userId,
        IReadOnlyCollection<string> revokedDeviceIds,
        IReadOnlyCollection<DeviceRevocationSessionTarget> sessionsWithRevokedKeys,
        Func<CancellationToken, Task>? persistAndCommitAsync,
        CancellationToken ct)
    {
        using var registrationFence = _enforcement.FenceNewConnections(
            userId,
            revokedDeviceIds);






        var currentTargetCandidates = FilterCurrentRuntimeTargets(
            sessionsWithRevokedKeys);
        var affectedSessions = _enforcement.GetAffectedSessionIds(
            userId,
            revokedDeviceIds,
            currentTargetCandidates
                .Select(target => target.SessionId)
                .ToList());
        await using var lifecycles = await _sessionEndAuthority.AcquireAsync(
            affectedSessions,
            ct);
        if (persistAndCommitAsync is not null)
        {
            await persistAndCommitAsync(ct);
        }
        var currentTargets = await GetCurrentLiveTargetsAsync(
            sessionsWithRevokedKeys,
            ct);
        _enforcement.EnforceCommitted(
            userId,
            revokedDeviceIds,
            currentTargets.Select(target => target.SessionId).ToList(),
            CloseReason.AccessRevoked);
    }

    private IReadOnlyList<DeviceRevocationSessionTarget> FilterCurrentRuntimeTargets(
        IReadOnlyCollection<DeviceRevocationSessionTarget> targets) =>
        targets
            .Distinct()
            .Where(target =>
            {
                var runtime = _runtimes.TryGet(target.SessionId);
                return runtime is null
                    || runtime.Host.SessionIncarnationId == target.IncarnationId;
            })
            .OrderBy(target => target.SessionId.Value)
            .ThenBy(target => target.IncarnationId)
            .ToList();

    private async Task<IReadOnlyList<DeviceRevocationSessionTarget>>
        GetCurrentLiveTargetsAsync(
            IReadOnlyCollection<DeviceRevocationSessionTarget> targets,
            CancellationToken ct)
    {
        var durableMatches = await _sessionResolver.GetCurrentTargetsAsync(
            targets,
            ct);
        return durableMatches
            .Where(target =>
            {
                var runtime = _runtimes.TryGet(target.SessionId);
                return runtime is null
                    || runtime.Host.SessionIncarnationId == target.IncarnationId;
            })
            .ToList();
    }

    public void ReportEnforcementPending(Guid revocationId, Exception exception) =>
        _logger.LogWarning(
            exception,
            "Device revocation enforcement marker remains pending revocation={RevocationId}",
            revocationId);

    public void PublishChanged(
        IReadOnlyCollection<UserId> audience,
        UserId userId,
        long generation) =>
        _userEvents.PublishDeviceListChanged(
            DiscoveryAudience.ForUsers(audience),
            userId,
            generation);
}
