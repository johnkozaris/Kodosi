using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SemanticReceiptAckVerifier(
    IUserDeviceRepository devices,
    IUserDeviceListRepository deviceLists,
    IPopSignatureVerifier signatureVerifier,
    TimeProvider timeProvider)
{
    public async Task<bool> VerifyAsync(
        SessionId sessionId,
        Guid incarnationId,
        Guid requestId,
        UserId authenticatedUserId,
        string authenticatedDeviceId,
        string requesterUserId,
        string requesterDeviceId,
        string signatureBase64,
        CancellationToken ct = default)
    {
        if (sessionId.Value == Guid.Empty
            || authenticatedUserId.Value == Guid.Empty
            || !Guid.TryParse(requesterUserId, out var requesterUserGuid)
            || UserId.From(requesterUserGuid) != authenticatedUserId
            || !string.Equals(
                requesterDeviceId,
                authenticatedDeviceId,
                StringComparison.Ordinal)
            || incarnationId == Guid.Empty
            || requestId == Guid.Empty
            || string.IsNullOrWhiteSpace(signatureBase64))
        {
            return false;
        }

        byte[] signature;
        try
        {
            signature = Convert.FromBase64String(signatureBase64);
        }
        catch (FormatException)
        {
            return false;
        }
        if (signature.Length == 0)
        {
            return false;
        }

        var device = await devices.GetByDeviceIdAsync(authenticatedDeviceId, ct);
        var deviceList = await deviceLists.GetLatestAsync(authenticatedUserId, ct);
        if (!ActiveDeviceAuthorization.IsAuthorized(
                device,
                deviceList,
                authenticatedUserId,
                timeProvider.GetUtcNow()))
        {
            return false;
        }

        var preimage = SemanticReceiptAckPreimage.Create(
            sessionId,
            incarnationId,
            requestId,
            authenticatedUserId,
            authenticatedDeviceId);
        return signatureVerifier.Verify(
            device!.SigningPublicKey,
            preimage,
            signature);
    }
}
