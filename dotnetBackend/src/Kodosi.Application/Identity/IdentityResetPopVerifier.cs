using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class IdentityResetPopVerifier(
    PopChallengeConsumer popConsumer,
    IAuthMetrics metrics)
{



    private const string MetricsEndpointTag = "identity_reset";

    private readonly PopChallengeConsumer _popConsumer = popConsumer;
    private readonly IAuthMetrics _metrics = metrics;

    public async Task VerifyAsync(
        UserId userId,
        IdentityResetPopPayload? payload,
        IReadOnlyList<UserDevice> activeEnrolled,
        CancellationToken ct = default)
    {
        if (payload is null
            || payload.ChallengeId == Guid.Empty
            || string.IsNullOrWhiteSpace(payload.SignerDeviceId)
            || string.IsNullOrWhiteSpace(payload.PopSignature))
        {
            throw new DeviceEnrollmentException(
                "Identity reset requires a signed challenge from an enrolled device.");
        }



        var signerDevice = activeEnrolled.FirstOrDefault(
            d => string.Equals(d.DeviceId, payload.SignerDeviceId, StringComparison.Ordinal));
        if (signerDevice is null)
        {
            _metrics.RecordPopFailure(PopFailureReason.SignerNotEnrolled, MetricsEndpointTag);
            throw new DeviceEnrollmentException(
                "Signer device is not an enrolled, active device of the authenticated user.");
        }

        byte[] popSignatureBytes;
        try
        {
            popSignatureBytes = Convert.FromBase64String(payload.PopSignature);
        }
        catch (FormatException)
        {
            throw new DeviceEnrollmentException("PopSignature must be valid base64.");
        }

        await _popConsumer.ConsumeAsync(
            userId,
            payload.ChallengeId,
            popSignatureBytes,
            signerDevice.SigningPublicKey,
            MetricsEndpointTag,
            ct);
    }
}
