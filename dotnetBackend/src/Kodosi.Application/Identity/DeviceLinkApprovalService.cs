using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class DeviceLinkApprovalService(
    IDeviceLinkRequestRepository linkRepository,
    IUserDeviceRepository deviceRepository,
    IUserDeviceListRepository listRepository,
    IUserRepository userRepository,
    IDeviceEnrollmentVerifier enrollmentVerifier,
    IUserLifecycleLock userLifecycleLock,
    IUnitOfWork unitOfWork,
    IDeviceLinkRealtimeEffects realtimeEffects,
    TimeProvider? timeProvider = null)
{
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    public async Task<bool> ApproveAsync(
        UserId userId,
        string userCode,
        byte[] certificate,
        byte[] certificateSignature,
        byte[] signedDeviceList,
        byte[] signedDeviceListSignature,
        CancellationToken ct = default)
    {
        var normalizedUserCode = userCode.Trim().ToUpperInvariant();
        await using var transaction = await unitOfWork.BeginTransactionAsync(ct);
        await userLifecycleLock.AcquireAsync(userId, ct);
        var row = await linkRepository.GetByUserCodeAsync(normalizedUserCode, ct);
        var now = _timeProvider.GetUtcNow();
        if (row is null || row.UserId != userId || !row.IsPending(now))
        {
            return false;
        }

        var currentList = await listRepository.GetLatestAsync(userId, ct);
        var user = await userRepository.GetByIdAsync(userId, ct)
            ?? throw new NotFoundException("User", userId.Value);
        RequireExistingIdentity(user, currentList);

        var verified = await enrollmentVerifier.VerifyAsync(
            userId,
            row.DeviceId,
            row.KemPublicKey,
            row.SigningPublicKey,
            certificate,
            certificateSignature,
            signedDeviceList,
            signedDeviceListSignature,
            ct);
        if (await deviceRepository.GetByDeviceIdAsync(row.DeviceId, ct) is not null)
        {
            throw new ConflictException("Device already enrolled.");
        }

        var device = UserDevice.CreateCertified(
            userId,
            row.DeviceId,
            certificate,
            certificateSignature,
            verified.AuthorizedAt);
        var parsedList = verified.DeviceList;
        var deviceList = UserDeviceList.Create(
            userId,
            parsedList.Generation,
            signedDeviceList,
            signedDeviceListSignature);

        row.Approve(parsedList.Generation, now);
        linkRepository.Update(row);
        await deviceRepository.AddAsync(device, ct);
        await listRepository.AddAsync(deviceList, ct);
        await linkRepository.InvalidateApprovedBeforeGenerationAsync(
            userId,
            parsedList.Generation,
            row.Id,
            now,
            ct);
        await unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);

        await realtimeEffects.PublishApprovedAsync(
            userId,
            parsedList.Generation,
            normalizedUserCode);
        return true;
    }

    private static void RequireExistingIdentity(User user, UserDeviceList? currentList)
    {
        if (currentList is null
            || user.IdentityRevision <= 0
            || user.IdentityIncarnationId is null)
        {
            throw new DeviceEnrollmentException(
                "Device-link approval requires an existing enrolled signer; bootstrap through device registration first.");
        }
    }
}
