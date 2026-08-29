namespace Kodosi.Application;

public interface IAuthMetrics
{
    void RecordPopFailure(PopFailureReason reason, string endpoint);
}

public enum PopFailureReason
{
    ChallengeExpired,
    SignatureInvalid,
    SignerNotEnrolled,
}
