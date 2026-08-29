using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record VerifiedDeviceLinkEnrollment(
    SignedDeviceListParser.ParsedSignedDeviceList DeviceList,
    DateTimeOffset AuthorizedAt);

public interface IDeviceEnrollmentVerifier
{
    Task<VerifiedDeviceLinkEnrollment> VerifyAsync(
        UserId userId,
        string deviceId,
        byte[] kemPublicKey,
        byte[] signingPublicKey,
        byte[] certificate,
        byte[] certificateSignature,
        byte[] signedDeviceList,
        byte[] signedDeviceListSignature,
        CancellationToken ct = default);
}

public interface IDeviceLinkRealtimeEffects
{
    Task PublishApprovedAsync(
        UserId userId,
        long generation,
        string userCode);
}

public enum DeviceLinkPollState
{
    NotFound,
    Pending,
    Cancelled,
    Expired,
    Approved,
}

public sealed record DeviceLinkPollResult(
    DeviceLinkPollState State,
    long? DeviceListGeneration = null,
    UserIdentityBundleResponse? IdentityBundle = null);
