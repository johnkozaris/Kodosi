using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class PopChallengeConsumer(
    IDeviceRegistrationChallengeRepository challenges,
    IPopSignatureVerifier verifier,
    IAuthMetrics metrics)
{
    private readonly IDeviceRegistrationChallengeRepository _challenges = challenges;
    private readonly IPopSignatureVerifier _verifier = verifier;
    private readonly IAuthMetrics _metrics = metrics;

    public async Task ConsumeAsync(
        UserId userId,
        Guid challengeId,
        byte[] popSignatureBytes,
        byte[] signerPublicKey,
        string endpointTag,
        CancellationToken ct = default)
    {
        var challenge = await _challenges.GetByIdAsync(challengeId, ct);
        if (challenge is null)
        {
            _metrics.RecordPopFailure(PopFailureReason.ChallengeExpired, endpointTag);
            throw new DeviceEnrollmentException("Challenge not found.");
        }
        if (challenge.UserId != userId)
        {
            _metrics.RecordPopFailure(PopFailureReason.ChallengeExpired, endpointTag);
            throw new DeviceEnrollmentException("Challenge does not belong to the authenticated user.");
        }
        if (!challenge.IsValid(DateTimeOffset.UtcNow))
        {
            _metrics.RecordPopFailure(PopFailureReason.ChallengeExpired, endpointTag);
            throw new DeviceEnrollmentException("Challenge has expired or already been used.");
        }


        if (!await _challenges.TryConsumeAsync(challenge, ct))
        {
            _metrics.RecordPopFailure(PopFailureReason.ChallengeExpired, endpointTag);
            throw new DeviceEnrollmentException("Challenge has already been consumed.");
        }

        var popTag = DomainTags.DevicePopV1;
        var preimage = new byte[popTag.Length + challenge.Challenge.Length];
        popTag.CopyTo(preimage);
        challenge.Challenge.CopyTo(preimage.AsSpan(popTag.Length));
        if (!_verifier.Verify(signerPublicKey, preimage, popSignatureBytes))
        {
            _metrics.RecordPopFailure(PopFailureReason.SignatureInvalid, endpointTag);
            throw new DeviceEnrollmentException("Proof-of-possession signature is invalid.");
        }
    }
}
