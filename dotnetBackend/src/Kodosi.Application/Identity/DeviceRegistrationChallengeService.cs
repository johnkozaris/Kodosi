using System.Security.Cryptography;
using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class DeviceRegistrationChallengeService(
    IDeviceRegistrationChallengeRepository challenges,
    IUnitOfWork unitOfWork)
{
    private const int ChallengeByteLength = 32;
    private static readonly TimeSpan ChallengeTtl = TimeSpan.FromMinutes(5);

    public async Task<DeviceRegistrationChallengeResult> CreateAsync(
        UserId userId,
        CancellationToken ct = default)
    {
        var challengeBytes = RandomNumberGenerator.GetBytes(ChallengeByteLength);
        var challenge = DeviceRegistrationChallenge.Create(
            userId,
            challengeBytes,
            ChallengeTtl);
        await challenges.AddAsync(challenge, ct);
        await unitOfWork.SaveChangesAsync(ct);
        return new DeviceRegistrationChallengeResult(
            challenge.Id,
            challengeBytes,
            challenge.ExpiresAt);
    }
}

public sealed record DeviceRegistrationChallengeResult(
    Guid Id,
    byte[] Challenge,
    DateTimeOffset ExpiresAt);
