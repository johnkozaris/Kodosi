using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class DeviceListReplacementService(
    IUserDeviceRepository deviceRepository,
    IUserDeviceListRepository listRepository,
    IDeviceLinkRequestRepository linkRequests,
    IRevokedDeviceInvitationCancellation invitationCancellation,
    IUserRepository userRepository,
    ISessionKeyBlobRepository blobRepository,
    ISemanticRelayLifecycleRepository semanticRelayLifecycle,
    IFriendshipRepository friendshipRepository,
    IRoomMemberRepository roomMemberRepository,
    IIdentityExposureRepository identityExposures,
    IDeviceRevocationAuditRepository revocationAudit,
    ISignedDeviceListVerifier signedListVerifier,
    IUserLifecycleLock userLifecycleLock,
    IUnitOfWork unitOfWork,
    IDeviceListRealtimeEffects realtimeEffects,
    IDeviceRevocationDurabilityCoordinator revocationDurability,
    IAuthMetrics authMetrics,
    TimeProvider timeProvider)
{
    private const long IssuedAtClockSkewMs = 5 * 60 * 1000;
    private const string MetricsEndpointTag = "device_list_replacement";

    public async Task ReplaceAsync(
        UserId userId,
        byte[] listBytes,
        byte[] listSignatureBytes,
        RequestAuditContext auditContext,
        CancellationToken ct = default)
    {
        var parsedList = signedListVerifier.Parse(listBytes);
        if (!Guid.TryParse(parsedList.UserId, out var listUserGuid)
            || listUserGuid != userId.Value)
        {
            throw new DeviceEnrollmentException(
                "Signed device list user_id does not match authenticated user.");
        }

        await using var transaction = await unitOfWork.BeginTransactionAsync(ct);
        await userLifecycleLock.AcquireAsync(userId, ct);

        var currentList = await listRepository.GetLatestAsync(userId, ct)
            ?? throw new DeviceEnrollmentException(
                "No existing device list; register a device before submitting revocations.");
        var identityRevision = (await userRepository.GetByIdAsync(userId, ct))?.IdentityRevision
            ?? throw new DeviceEnrollmentException("Authenticated user identity does not exist.");
        if (parsedList.Generation != currentList.Generation + 1)
        {
            throw new DeviceEnrollmentException(
                $"Signed list generation {parsedList.Generation} must equal current generation + 1 ({currentList.Generation + 1}).");
        }
        var currentProof = currentList.ParseBody();
        if (parsedList.IssuedAtMs <= currentProof.IssuedAtMs)
        {
            throw new DeviceEnrollmentException(
                "Signed list IssuedAtMs must be greater than the current generation's IssuedAtMs.");
        }
        var now = timeProvider.GetUtcNow();
        if (parsedList.IssuedAtMs > now.ToUnixTimeMilliseconds() + IssuedAtClockSkewMs)
        {
            throw new DeviceEnrollmentException(
                "Signed list IssuedAtMs is too far in the future; check your system clock.");
        }

        if (parsedList.ExpiresAtMs is { } expiresAtMs
            && expiresAtMs <= now.ToUnixTimeMilliseconds())
        {
            throw new DeviceEnrollmentException(
                "Signed device list is expired at authorization time.");
        }

        var signerDevice = await deviceRepository.GetByDeviceIdAsync(parsedList.SignerDeviceId, ct);
        if (!ActiveDeviceAuthorization.IsAuthorized(signerDevice, currentList, userId, now)
            || signerDevice!.SigningPublicKey is not { Length: > 0 })
        {
            authMetrics.RecordPopFailure(PopFailureReason.SignerNotEnrolled, MetricsEndpointTag);
            throw new DeviceEnrollmentException(
                "Signer device is not an enrolled, active device of the authenticated user.");
        }
        var previousEntryIds = currentProof.Entries
            .Select(entry => entry.DeviceId)
            .ToHashSet(StringComparer.Ordinal);
        if (!previousEntryIds.Contains(signerDevice.DeviceId))
        {
            authMetrics.RecordPopFailure(PopFailureReason.SignerNotEnrolled, MetricsEndpointTag);
            throw new DeviceEnrollmentException(
                "Signer device is not in the current generation's entry set.");
        }
        if (!signedListVerifier.Verify(listBytes, listSignatureBytes, signerDevice.SigningPublicKey))
        {
            authMetrics.RecordPopFailure(PopFailureReason.SignatureInvalid, MetricsEndpointTag);
            throw new DeviceEnrollmentException("Signed device list signature is invalid.");
        }

        var newDeviceIds = parsedList.Entries
            .Select(entry => entry.DeviceId)
            .ToHashSet(StringComparer.Ordinal);
        var existingDevices = await deviceRepository.GetByUserIdAsync(userId, ct);
        var existingByDeviceId = existingDevices
            .ToDictionary(device => device.DeviceId, StringComparer.Ordinal);
        DeviceListEntrySet.RequireCertificateSignerContinuity(
            parsedList.Entries.Select(entry => (entry.DeviceId, entry.SignerDeviceId)),
            existingByDeviceId);
        foreach (var entry in parsedList.Entries)
        {
            if (!existingByDeviceId.TryGetValue(entry.DeviceId, out var existing)
                || existing.UserId != userId)
            {
                throw new DeviceEnrollmentException(
                    "Signed list references a device that is not enrolled to this user; additions must use the registration flow.");
            }
            if (existing.RevokedAt is not null)
            {
                throw new DeviceEnrollmentException(
                    "Signed list cannot reintroduce a revoked device; reset trust before establishing a new identity.");
            }
        }

        var revokedNow = existingDevices
            .Where(device => device.RevokedAt is null && !newDeviceIds.Contains(device.DeviceId))
            .ToList();
        var revokedDeviceIds = revokedNow
            .Select(device => device.DeviceId)
            .Order(StringComparer.Ordinal)
            .ToArray();
        if (!newDeviceIds.Contains(signerDevice.DeviceId))
        {
            throw new DeviceEnrollmentException("Signer device cannot revoke itself in the same list.");
        }
        foreach (var device in revokedNow)
        {
            device.Revoke(signerDevice.DeviceId);
            deviceRepository.Update(device);
        }

        IReadOnlyList<UserId> cancelledInvitationRecipients = [];
        if (revokedDeviceIds.Length > 0)
        {
            cancelledInvitationRecipients = await invitationCancellation.CancelPendingAsync(
                userId, revokedDeviceIds, now, ct);
        }

        var deviceList = UserDeviceList.Create(
            userId,
            parsedList.Generation,
            listBytes,
            listSignatureBytes);
        await listRepository.AddAsync(deviceList, ct);
        await linkRequests.InvalidateApprovedBeforeGenerationAsync(
            userId,
            parsedList.Generation,
            null,
            now,
            ct);

        var revocationId = Guid.NewGuid();
        IReadOnlyList<DeviceRevocationSessionTarget> sessionsWithRevokedKeys = [];
        if (revokedNow.Count > 0)
        {
            sessionsWithRevokedKeys =
                await blobRepository.GetSessionTargetsForRecipientDevicesAsync(
                    revokedDeviceIds,
                    ct);
        }
        var blobsCascaded = 0;
        async Task PersistAndCommitAsync(CancellationToken persistenceCt)
        {
            if (revokedNow.Count > 0)
            {
                blobsCascaded = await blobRepository.DeleteForRecipientDevicesAsync(
                    revokedDeviceIds,
                    persistenceCt);
                await semanticRelayLifecycle.DeleteForDevicesAsync(
                    userId,
                    revokedDeviceIds,
                    persistenceCt);
                foreach (var revoked in revokedNow)
                {
                    await revocationAudit.AddAsync(DeviceRevocationAuditEntry.Create(
                        revocationId,
                        userId,
                        revoked.DeviceId,
                        signerDevice.DeviceId,
                        parsedList.Generation,
                        identityRevision,
                        blobsCascaded,
                        sessionsWithRevokedKeys,
                        auditContext.ClientIp,
                        auditContext.UserAgent));
                }
            }
            if (parsedList.ExpiresAtMs is { } finalExpiry
                && finalExpiry <= timeProvider.GetUtcNow().ToUnixTimeMilliseconds())
            {
                throw new DeviceEnrollmentException(
                    "Signed device list expired before durable acceptance.");
            }
            await unitOfWork.SaveChangesAsync(persistenceCt);
            await transaction.CommitAsync(persistenceCt);
        }

        if (revokedNow.Count == 0)
        {
            await PersistAndCommitAsync(ct);
        }
        else
        {
            await realtimeEffects.PersistAndEnforceAsync(
                userId,
                revokedDeviceIds,
                sessionsWithRevokedKeys,
                PersistAndCommitAsync,
                ct);
        }

        if (revokedNow.Count > 0)
        {
            try
            {
                await revocationDurability.CompleteEnforcementAsync(
                    revocationId,
                    CancellationToken.None);
            }
            catch (Exception exception)
            {
                realtimeEffects.ReportEnforcementPending(revocationId, exception);
            }
        }

        if (cancelledInvitationRecipients.Count > 0)
        {
            realtimeEffects.PublishInvitationsChanged(
                cancelledInvitationRecipients.Append(userId).Distinct().ToList());
        }

        var friendIds = await friendshipRepository.GetFriendIdsAsync(userId);
        var roomPeerIds = await roomMemberRepository.GetRoomPeerUserIdsAsync(userId);
        var historicalPeers = await identityExposures.GetHistoricalPeerUserIdsAsync(
            userId,
            CancellationToken.None);
        realtimeEffects.PublishChanged(
            DeviceListAudience.Build(friendIds.Concat(historicalPeers), roomPeerIds, userId),
            userId,
            deviceList.Generation);
    }
}
