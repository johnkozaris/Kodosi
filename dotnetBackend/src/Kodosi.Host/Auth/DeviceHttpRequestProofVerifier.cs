using System.Security.Cryptography;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.Host.Auth;

internal sealed record AuthenticatedDevice(UserId UserId, string DeviceId);

internal sealed class DeviceHttpRequestProofVerifier(
    IDeviceRegistrationChallengeRepository challenges,
    IUserDeviceRepository devices,
    Realtime.RealtimeDeviceAuthorizationReader authorization,
    IPopSignatureVerifier signatures)
{
    internal const string DeviceIdHeader = "X-Kodosi-Device-Id";
    internal const string ChallengeIdHeader = "X-Kodosi-Device-Challenge-Id";
    internal const string SignatureHeader = "X-Kodosi-Device-Signature";
    internal const string BodySha256Header = "X-Kodosi-Body-Sha256";
    internal const string EmptyBodySha256 =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    public async Task<AuthenticatedDevice?> VerifyAsync(
        HttpRequest request,
        UserId userId,
        string expectedBodySha256,
        CancellationToken ct)
    {
        var deviceId = request.Headers[DeviceIdHeader].ToString().Trim();
        var signatureText = request.Headers[SignatureHeader].ToString().Trim();
        if (deviceId.Length == 0
            || !Guid.TryParse(request.Headers[ChallengeIdHeader], out var challengeId)
            || signatureText.Length == 0
            || !await HasExpectedBodyAsync(request, expectedBodySha256, ct))
        {
            return null;
        }

        var challenge = await challenges.GetByIdAsync(challengeId, ct);
        if (challenge is null
            || challenge.UserId != userId
            || !challenge.IsValid(DateTimeOffset.UtcNow))
        {
            return null;
        }



        if (!await challenges.TryConsumeAsync(challenge, ct))
        {
            return null;
        }

        byte[] signature;
        try
        {
            signature = Convert.FromBase64String(signatureText);
        }
        catch (FormatException)
        {
            return null;
        }

        var decision = await authorization.EvaluateAsync(userId, deviceId, ct);
        if (!decision.Authorized)
        {
            return null;
        }
        var device = await devices.GetByDeviceIdAsync(deviceId, ct);
        if (device is null || device.UserId != userId || device.RevokedAt.HasValue)
        {
            return null;
        }

        var suppliedBodyHash = request.Headers[BodySha256Header].ToString().Trim().ToLowerInvariant();
        if (!CryptographicOperations.FixedTimeEquals(
                System.Text.Encoding.ASCII.GetBytes(suppliedBodyHash),
                System.Text.Encoding.ASCII.GetBytes(expectedBodySha256.ToLowerInvariant())))
        {
            return null;
        }
        var bodyHash = suppliedBodyHash;
        var canonicalPathAndQuery = request.PathBase.Add(request.Path) + request.QueryString;
        byte[] preimage;
        try
        {
            preimage = DeviceHttpRequestProofPreimage.Create(
                userId,
                deviceId,
                challengeId,
                request.Method,
                canonicalPathAndQuery,
                bodyHash,
                challenge.Challenge);
        }
        catch (DomainException)
        {
            return null;
        }
        return signatures.Verify(device.SigningPublicKey, preimage, signature)
            ? new AuthenticatedDevice(userId, deviceId)
            : null;
    }

    internal static async Task<bool> HasExpectedBodyAsync(
        HttpRequest request,
        string expectedBodySha256,
        CancellationToken ct)
    {
        if (!string.Equals(
                expectedBodySha256,
                EmptyBodySha256,
                StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }
        if (request.ContentLength is > 0)
        {
            return false;
        }

        var probe = new byte[1];
        return await request.Body.ReadAsync(probe, ct) == 0;
    }
}
