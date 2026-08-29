using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record SemanticReceiptEnvelope(
    SessionId SessionId,
    Guid IncarnationId,
    Guid RequestId,
    string Mode,
    string PayloadSha256,
    string Outcome,
    UserId RequesterUserId,
    string RequesterDeviceId,
    UserId OwnerUserId,
    string OwnerDeviceId,
    string Signature);

public sealed record SemanticReceiptAcknowledgement(
    SessionId SessionId,
    Guid IncarnationId,
    Guid RequestId,
    string RequesterUserId,
    string RequesterDeviceId,
    string Signature);

public enum SemanticReceiptUploadOutcome
{
    Stored,
    Conflict,
    UnauthorizedDevice,
}

public enum SemanticReceiptAckOutcome
{
    Acknowledged,
    NotFound,
    InvalidSignature,
    UnauthorizedDevice,
}

public sealed class SemanticReceiptMailboxService(
    ISemanticRelayRepository repository,
    ISemanticReceiptVerifier receiptVerifier,
    SemanticReceiptAckVerifier ackVerifier,
    IUserDeviceRepository devices,
    IUserDeviceListRepository deviceLists,
    TimeProvider timeProvider)
{
    public async Task<SemanticReceiptUploadOutcome> UploadAsync(
        UserId authenticatedUserId,
        string authenticatedDeviceId,
        SemanticReceiptEnvelope receipt,
        CancellationToken ct = default)
    {
        if (!await receiptVerifier.VerifyAsync(
                authenticatedUserId,
                authenticatedDeviceId,
                receipt,
                ct))
        {
            return SemanticReceiptUploadOutcome.UnauthorizedDevice;
        }

        var stored = await repository.StoreReceiptAsync(
            receipt.SessionId,
            receipt.IncarnationId,
            receipt.RequesterUserId,
            receipt.RequesterDeviceId,
            receipt.RequestId,
            receipt.Mode,
            receipt.PayloadSha256,
            receipt.Outcome,
            receipt.OwnerUserId,
            receipt.OwnerDeviceId,
            receipt.Signature,
            ct);
        return stored
            ? SemanticReceiptUploadOutcome.Stored
            : SemanticReceiptUploadOutcome.Conflict;
    }

    public async Task<SemanticReceiptPage?> ListAsync(
        UserId authenticatedUserId,
        string authenticatedDeviceId,
        int limit,
        string? cursor,
        CancellationToken ct = default)
    {
        if (!await IsActiveDeviceAsync(authenticatedUserId, authenticatedDeviceId, ct))
        {
            return null;
        }
        var decodedCursor = SemanticReceiptCursor.Decode(cursor);
        var receipts = await repository.ListPendingReceiptsForDeviceAsync(
            authenticatedUserId,
            authenticatedDeviceId,
            checked(limit + 1),
            decodedCursor,
            ct);
        var hasMore = receipts.Count > limit;
        var items = hasMore ? receipts.Take(limit).ToArray() : receipts;
        var nextCursor = hasMore
            ? new SemanticReceiptCursor(items[^1].StoredAt, items[^1].Id).Encode()
            : null;
        return new SemanticReceiptPage(items, nextCursor);
    }

    public async Task<SemanticReceiptAckOutcome> AcknowledgeAsync(
        UserId authenticatedUserId,
        string authenticatedDeviceId,
        SemanticReceiptAcknowledgement acknowledgement,
        CancellationToken ct = default)
    {
        if (!string.Equals(
                authenticatedDeviceId,
                acknowledgement.RequesterDeviceId,
                StringComparison.Ordinal))
        {
            return SemanticReceiptAckOutcome.UnauthorizedDevice;
        }
        if (!await ackVerifier.VerifyAsync(
                acknowledgement.SessionId,
                acknowledgement.IncarnationId,
                acknowledgement.RequestId,
                authenticatedUserId,
                authenticatedDeviceId,
                acknowledgement.RequesterUserId,
                acknowledgement.RequesterDeviceId,
                acknowledgement.Signature,
                ct))
        {
            return SemanticReceiptAckOutcome.InvalidSignature;
        }
        return await repository.AcknowledgeReceiptAsync(
            acknowledgement.SessionId,
            acknowledgement.IncarnationId,
            authenticatedUserId,
            authenticatedDeviceId,
            acknowledgement.RequestId,
            ct)
            ? SemanticReceiptAckOutcome.Acknowledged
            : SemanticReceiptAckOutcome.NotFound;
    }

    private async Task<UserDevice?> GetActiveDeviceAsync(
        UserId userId,
        string deviceId,
        CancellationToken ct)
    {
        if (string.IsNullOrWhiteSpace(deviceId))
        {
            return null;
        }
        var device = await devices.GetByDeviceIdAsync(deviceId, ct);
        var deviceList = await deviceLists.GetLatestAsync(userId, ct);
        return ActiveDeviceAuthorization.IsAuthorized(
            device,
            deviceList,
            userId,
            timeProvider.GetUtcNow())
            ? device
            : null;
    }

    private async Task<bool> IsActiveDeviceAsync(
        UserId userId,
        string deviceId,
        CancellationToken ct)
    {
        return await GetActiveDeviceAsync(userId, deviceId, ct) is not null;
    }
}
