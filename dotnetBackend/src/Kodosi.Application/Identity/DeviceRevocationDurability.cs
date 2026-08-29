using Kodosi.Domain;

namespace Kodosi.Application;

public interface IDeviceRevocationDurabilityCoordinator
{
    Task<IReadOnlyList<DeviceRevocationEnforcementWork>> GetPendingEnforcementAsync(
        int limit,
        CancellationToken ct = default);

    Task<bool> ExecuteIfCurrentAsync(
        DeviceRevocationEnforcementWork work,
        Func<CancellationToken, Task> enforce,
        CancellationToken ct = default);

    Task CompleteEnforcementAsync(
        Guid revocationId,
        CancellationToken ct = default);
}

public interface IDeviceRevocationSessionResolver
{
    Task<IReadOnlyList<DeviceRevocationSessionTarget>> GetCurrentTargetsAsync(
        IReadOnlyCollection<DeviceRevocationSessionTarget> targets,
        CancellationToken ct = default);
}

public sealed record DeviceRevocationEnforcementWork(
    Guid RevocationId,
    UserId UserId,
    long IdentityRevision,
    long DeviceListGeneration,
    IReadOnlyList<string> RevokedDeviceIds,
    IReadOnlyList<DeviceRevocationSessionTarget> AffectedSessionTargets);
