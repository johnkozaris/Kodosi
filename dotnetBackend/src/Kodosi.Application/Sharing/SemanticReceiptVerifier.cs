using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISemanticReceiptVerifier
{
    Task<bool> VerifyAsync(
        UserId authenticatedUserId,
        string authenticatedDeviceId,
        SemanticReceiptEnvelope receipt,
        CancellationToken ct = default);
}

public sealed class SemanticReceiptVerifier(
    IUserDeviceRepository devices,
    IUserDeviceListRepository deviceLists,
    IPopSignatureVerifier signatureVerifier,
    TimeProvider timeProvider) : ISemanticReceiptVerifier
{
    public async Task<bool> VerifyAsync(
        UserId authenticatedUserId,
        string authenticatedDeviceId,
        SemanticReceiptEnvelope receipt,
        CancellationToken ct = default)
    {
        if (authenticatedUserId != receipt.OwnerUserId
            || !string.Equals(authenticatedDeviceId, receipt.OwnerDeviceId, StringComparison.Ordinal)
            || receipt.RequesterUserId != receipt.OwnerUserId
            || string.IsNullOrWhiteSpace(authenticatedDeviceId))
        {
            return false;
        }

        byte[] signature;
        try
        {
            signature = Convert.FromBase64String(receipt.Signature);
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

        byte[] preimage;
        try
        {
            preimage = SemanticReceiptPreimage.Create(
                receipt.SessionId,
                receipt.IncarnationId,
                receipt.RequestId,
                receipt.Mode,
                receipt.PayloadSha256,
                receipt.Outcome,
                receipt.RequesterUserId,
                receipt.RequesterDeviceId,
                receipt.OwnerUserId,
                receipt.OwnerDeviceId);
        }
        catch (DomainException)
        {
            return false;
        }
        return signatureVerifier.Verify(
            device!.SigningPublicKey,
            preimage,
            signature);
    }
}
