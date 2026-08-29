using Kodosi.Domain;

namespace Kodosi.Application;

public interface IDeviceRegistrationChallengeRepository
{
    Task AddAsync(DeviceRegistrationChallenge challenge, CancellationToken ct = default);

    Task<DeviceRegistrationChallenge?> GetByIdAsync(Guid id, CancellationToken ct = default);

    Task<bool> TryConsumeAsync(
        DeviceRegistrationChallenge challenge,
        CancellationToken ct = default);

    Task<int> DeleteExpiredAsync(
        DateTimeOffset now,
        int limit,
        CancellationToken ct = default);
}
