using Kodosi.Application;

namespace Kodosi.Host.Observability;

internal sealed class AuthMetricsAdapter(OperationalMetrics metrics) : IAuthMetrics
{
    private readonly OperationalMetrics _metrics = metrics;

    public void RecordPopFailure(PopFailureReason reason, string endpoint)
    {
        _metrics.RecordPopFailure(ReasonTag(reason), endpoint);
    }



    private static string ReasonTag(PopFailureReason reason) => reason switch
    {
        PopFailureReason.ChallengeExpired => "challenge_expired",
        PopFailureReason.SignatureInvalid => "signature_invalid",
        PopFailureReason.SignerNotEnrolled => "signer_not_enrolled",
        _ => throw new ArgumentOutOfRangeException(
            nameof(reason),
            reason,
            "Unmapped proof-of-possession failure reason."),
    };
}
