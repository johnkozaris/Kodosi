
namespace Kodosi.Domain;

public sealed class DeviceRegistrationChallenge
{
    private const int ChallengeByteLength = 32;

    public Guid Id { get; private set; }
    public UserId UserId { get; private set; }
    public byte[] Challenge { get; private set; } = [];
    public DateTimeOffset ExpiresAt { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }


    public uint Version { get; private set; }

    private DeviceRegistrationChallenge() { }

    public static DeviceRegistrationChallenge Create(
        UserId userId,
        byte[] challenge,
        TimeSpan ttl)
    {
        if (challenge is not { Length: ChallengeByteLength })
            throw new DomainException($"Challenge must be exactly {ChallengeByteLength} bytes.");
        if (ttl <= TimeSpan.Zero)
            throw new DomainException("Challenge TTL must be positive.");

        var now = DateTimeOffset.UtcNow;
        return new DeviceRegistrationChallenge
        {
            Id = Guid.NewGuid(),
            UserId = userId,
            Challenge = [.. challenge],
            ExpiresAt = now + ttl,
            CreatedAt = now,
        };
    }

    public bool IsValid(DateTimeOffset now) => now < ExpiresAt;
}
