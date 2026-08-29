using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

internal sealed class FakeDeviceRevocationSessionResolver(
    params DeviceRevocationSessionTarget[] currentTargets)
    : IDeviceRevocationSessionResolver
{
    public List<IReadOnlyList<DeviceRevocationSessionTarget>> Calls { get; } = [];

    public Task<IReadOnlyList<DeviceRevocationSessionTarget>> GetCurrentTargetsAsync(
        IReadOnlyCollection<DeviceRevocationSessionTarget> targets,
        CancellationToken ct = default)
    {
        ct.ThrowIfCancellationRequested();
        Calls.Add(targets.ToList());
        var expected = targets.ToHashSet();
        return Task.FromResult<IReadOnlyList<DeviceRevocationSessionTarget>>(
            currentTargets.Where(expected.Contains).ToList());
    }
}
