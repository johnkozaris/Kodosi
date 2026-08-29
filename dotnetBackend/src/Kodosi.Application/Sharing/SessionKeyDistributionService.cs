using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionKeyDistributionService(
    ISessionRepository sessions,
    ISessionKeyBlobRepository keyBlobs,
    IUserDeviceRepository devices,
    IUserDeviceListRepository deviceLists,
    IPopSignatureVerifier signatureVerifier,
    IUserLifecycleLock userLifecycleLock,
    IRecipientDeviceLifecycleLock recipientDeviceLifecycleLock,
    ISessionEndAuthority sessionEndAuthority,
    SessionAccessService sessionAccess,
    IUnitOfWork unitOfWork,
    ISharingMetrics metrics,
    TimeProvider timeProvider)
{
    private readonly ISessionRepository _sessions = sessions;
    private readonly ISessionKeyBlobRepository _keyBlobs = keyBlobs;
    private readonly IUserDeviceRepository _devices = devices;
    private readonly IUserDeviceListRepository _deviceLists = deviceLists;
    private readonly IPopSignatureVerifier _signatureVerifier = signatureVerifier;
    private readonly IUserLifecycleLock _userLifecycleLock = userLifecycleLock;
    private readonly IRecipientDeviceLifecycleLock _recipientDeviceLifecycleLock =
        recipientDeviceLifecycleLock;
    private readonly ISessionEndAuthority _sessionEndAuthority = sessionEndAuthority;
    private readonly SessionAccessService _sessionAccess = sessionAccess;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;
    private readonly ISharingMetrics _metrics = metrics;
    private readonly TimeProvider _timeProvider = timeProvider;

    public async Task<StoreSessionKeyBlobsResult> ReplaceKeyBlobsAsync(
        SessionId sessionId,
        UserId actorUserId,
        Guid expectedIncarnationId,
        IReadOnlyList<SessionKeyBlobSubmission> submissions,
        CancellationToken ct = default)
    {
        var recipientDeviceIds = new HashSet<string>(StringComparer.Ordinal);
        foreach (var submission in submissions)
        {
            if (string.IsNullOrWhiteSpace(submission.RecipientDeviceId))
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    "Recipient device ID is required.");
            }
            if (!recipientDeviceIds.Add(submission.RecipientDeviceId.Trim()))
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"Duplicate recipient device ID {submission.RecipientDeviceId.Trim()}.");
            }
        }

        var recipientUsersByDevice =
            await _devices.GetUserIdsByDeviceIdsAsync(recipientDeviceIds, ct);
        foreach (var recipientDeviceId in recipientDeviceIds)
        {
            if (!recipientUsersByDevice.ContainsKey(recipientDeviceId))
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"Recipient device {recipientDeviceId} is not active.");
            }
        }

        var recipientUserIds = recipientUsersByDevice.Values
            .Distinct()
            .OrderBy(userId => userId.Value)
            .ToList();
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _recipientDeviceLifecycleLock.AcquireAsync(
            recipientUserIds,
            ct);


        foreach (var userId in recipientUserIds
            .Append(actorUserId)
            .Distinct()
            .OrderBy(userId => userId.Value))
        {
            await _userLifecycleLock.AcquireAsync(userId, ct);
        }
        await using var sessionLifecycle = await _sessionEndAuthority.AcquireAsync(
            [sessionId],
            ct);

        var session = await _sessions.GetByIdForUpdateAsync(sessionId, ct);
        if (session is null || !session.IsOwner(actorUserId))
        {
            return StoreSessionKeyBlobsResult.NotFound();
        }

        if (session.IncarnationId != expectedIncarnationId)
        {
            return StoreSessionKeyBlobsResult.StaleIncarnation();
        }

        if (session.Status == SessionStatus.Ended)
        {
            return StoreSessionKeyBlobsResult.NotFound();
        }

        var deniedRecipientUserIds = await _sessionAccess.GetDeniedUserIdsAsync(
            session,
            recipientUserIds,
            ct);
        if (deniedRecipientUserIds.Count > 0)
        {
            var deniedDeviceId = recipientUsersByDevice
                .Where(pair => deniedRecipientUserIds.Contains(pair.Value))
                .Select(pair => pair.Key)
                .Order(StringComparer.Ordinal)
                .First();
            return StoreSessionKeyBlobsResult.InvalidRequest(
                $"Recipient device {deniedDeviceId} is not authorized for the session.");
        }

        var now = _timeProvider.GetUtcNow();
        var recipientDeviceLists = new Dictionary<UserId, UserDeviceList?>();
        foreach (var (recipientDeviceId, expectedUserId) in recipientUsersByDevice)
        {
            var recipientDevice =
                await _devices.GetByDeviceIdAsync(recipientDeviceId, ct);
            if (recipientDevice?.UserId != expectedUserId)
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"Recipient device {recipientDeviceId} is not active.");
            }

            if (!recipientDeviceLists.TryGetValue(
                    expectedUserId,
                    out var recipientDeviceList))
            {
                recipientDeviceList = await _deviceLists.GetLatestAsync(
                    expectedUserId,
                    ct);
                recipientDeviceLists[expectedUserId] = recipientDeviceList;
            }

            if (!ActiveDeviceAuthorization.IsAuthorized(
                    recipientDevice,
                    recipientDeviceList,
                    expectedUserId,
                    now))
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"Recipient device {recipientDeviceId} is not active.");
            }
        }

        if (submissions.Count == 0)
        {
            return StoreSessionKeyBlobsResult.InvalidRequest("At least one key blob is required.");
        }

        var blobs = new List<SessionKeyBlob>(submissions.Count);
        var senderDevices = new Dictionary<string, UserDevice>(StringComparer.Ordinal);
        var senderDeviceList = await _deviceLists.GetLatestAsync(actorUserId, ct);
        var nowMs = now.ToUnixTimeMilliseconds();
        foreach (var submission in submissions)
        {
            var recipientDeviceId = submission.RecipientDeviceId.Trim();

            if (string.IsNullOrWhiteSpace(submission.SenderDeviceId))
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"SenderDeviceId is required for device {recipientDeviceId}.");
            }
            var senderDeviceId = submission.SenderDeviceId.Trim();

            if (!TryDecodeBase64(submission.EncryptedSessionKey, out var encryptedKey))
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"Invalid base64 in encrypted key for device {recipientDeviceId}.");
            }

            if (string.IsNullOrWhiteSpace(submission.Signature))
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"Missing key blob signature for device {recipientDeviceId}.");
            }

            if (!TryDecodeBase64(submission.Signature, out var signature))
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"Invalid base64 in key blob signature for device {recipientDeviceId}.");
            }

            if (!senderDevices.TryGetValue(senderDeviceId, out var senderDevice))
            {
                senderDevice = await _devices.GetByDeviceIdAsync(senderDeviceId, ct);
                if (!ActiveDeviceAuthorization.IsAuthorized(
                        senderDevice,
                        senderDeviceList,
                        actorUserId,
                        now))
                {
                    return StoreSessionKeyBlobsResult.InvalidRequest(
                        $"Sender device {senderDeviceId} is not active.");
                }

                senderDevices[senderDeviceId] = senderDevice!;
            }

            if (submission.KeyGeneration < 0)
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    "KeyGeneration must be non-negative.");
            }

            if (submission.KeyGeneration != session.CurrentKeyGeneration)
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"KeyGeneration {submission.KeyGeneration} does not match the session's current generation {session.CurrentKeyGeneration}. "
                    + "Claim the next generation before distributing blobs.");
            }



            const long MaxSkewMs = 15 * 60 * 1000;
            if (submission.IssuedAtMs <= 0)
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    "IssuedAtMs must be a positive Unix epoch milliseconds value.");
            }
            if (submission.IssuedAtMs < nowMs - MaxSkewMs
                || submission.IssuedAtMs > nowMs + MaxSkewMs)
            {
                _metrics.RecordSessionKeySkewRejection();
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"IssuedAtMs drifted more than {MaxSkewMs}ms from backend clock; "
                    + "check the client's system clock.");
            }

            var signatureVersion = submission.SignatureVersion;
            byte[] signatureDigest;
            if (signatureVersion == SessionKeyBlobSignatureDigest.LegacyVersion)
            {
                if (session.IncarnationProtocolVersion
                    != Session.LegacyIncarnationProtocolVersion)
                {
                    return StoreSessionKeyBlobsResult.InvalidRequest(
                        "Legacy key blob signatures are allowed only for a legacy session incarnation.");
                }

                signatureDigest = SessionKeyBlobSignatureDigest.ComputeV1(
                    sessionId.ToString(),
                    recipientDeviceId,
                    encryptedKey,
                    checked((uint)submission.KeyGeneration),
                    checked((ulong)submission.IssuedAtMs));
            }
            else if (signatureVersion == SessionKeyBlobSignatureDigest.CurrentVersion)
            {
                signatureDigest = SessionKeyBlobSignatureDigest.ComputeV2(
                    sessionId.ToString(),
                    session.IncarnationId,
                    recipientDeviceId,
                    encryptedKey,
                    checked((uint)submission.KeyGeneration),
                    checked((ulong)submission.IssuedAtMs));
            }
            else
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"Unsupported key blob signature version {signatureVersion}.");
            }
            if (!_signatureVerifier.Verify(
                    senderDevice!.SigningPublicKey,
                    signatureDigest,
                    signature))
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(
                    $"Invalid key blob signature for device {recipientDeviceId}.");
            }

            try
            {
                blobs.Add(SessionKeyBlob.Create(
                    sessionId,
                    recipientDeviceId,
                    encryptedKey,
                    senderDeviceId,
                    senderDevice.KemPublicKey,
                    submission.KeyGeneration,
                    signatureVersion,
                    signature,
                    submission.IssuedAtMs));
            }
            catch (DomainException exception)
            {
                return StoreSessionKeyBlobsResult.InvalidRequest(exception.Message);
            }
        }

        await _keyBlobs.DeleteForSessionAsync(sessionId, ct);
        await _keyBlobs.AddRangeAsync(blobs, ct);
        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return StoreSessionKeyBlobsResult.Stored();
    }

    private static bool TryDecodeBase64(string? encodedValue, out byte[] bytes)
    {
        if (string.IsNullOrWhiteSpace(encodedValue))
        {
            bytes = [];
            return false;
        }

        try
        {
            bytes = Convert.FromBase64String(encodedValue);
            return true;
        }
        catch (FormatException)
        {
            bytes = [];
            return false;
        }
    }
}

public sealed record SessionKeyBlobSubmission(
    string RecipientDeviceId,
    string EncryptedSessionKey,
    string SenderDeviceId,
    int KeyGeneration,
    long IssuedAtMs,
    string? Signature,
    int SignatureVersion);

public sealed record StoreSessionKeyBlobsResult(
    StoreSessionKeyBlobsState State,
    string? ErrorMessage = null)
{
    public static StoreSessionKeyBlobsResult Stored()
        => new(StoreSessionKeyBlobsState.Stored);

    public static StoreSessionKeyBlobsResult NotFound()
        => new(StoreSessionKeyBlobsState.NotFound);

    public static StoreSessionKeyBlobsResult StaleIncarnation()
        => new(StoreSessionKeyBlobsState.StaleIncarnation);

    public static StoreSessionKeyBlobsResult InvalidRequest(string errorMessage)
        => new(StoreSessionKeyBlobsState.InvalidRequest, errorMessage);
}

public enum StoreSessionKeyBlobsState
{
    Stored,
    NotFound,
    StaleIncarnation,
    InvalidRequest,
}
