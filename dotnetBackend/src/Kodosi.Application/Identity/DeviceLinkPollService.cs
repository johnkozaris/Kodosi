using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class DeviceLinkPollService(
    IDeviceLinkRequestRepository linkRepository,
    IUserDeviceListRepository listRepository,
    IUserDeviceRepository deviceRepository,
    IUserRepository userRepository,
    IUserLifecycleLock userLifecycleLock,
    IUnitOfWork unitOfWork)
{
    public async Task<DeviceLinkPollResult> PollAsync(
        UserId userId,
        string deviceCode,
        CancellationToken ct = default)
    {
        if (string.IsNullOrWhiteSpace(deviceCode))
        {
            return new DeviceLinkPollResult(DeviceLinkPollState.NotFound);
        }

        await using var transaction = await unitOfWork.BeginTransactionAsync(ct);
        await userLifecycleLock.AcquireAsync(userId, ct);
        var result = await ReadAsync(userId, deviceCode, ct);
        if (result.State != DeviceLinkPollState.Approved)
        {
            await transaction.CommitAsync(ct);
            return result;
        }

        var bundle = await LoadApprovedBundleAsync(
            userId,
            result.DeviceListGeneration,
            ct);
        var revalidated = await ReadAsync(userId, deviceCode, ct);
        await transaction.CommitAsync(ct);
        if (revalidated.State != DeviceLinkPollState.Approved
            || revalidated.DeviceListGeneration != result.DeviceListGeneration)
        {
            return revalidated;
        }
        return revalidated with { IdentityBundle = bundle };
    }

    private async Task<DeviceLinkPollResult> ReadAsync(
        UserId userId,
        string deviceCode,
        CancellationToken ct)
    {
        var row = await linkRepository.GetByDeviceCodeAsync(deviceCode, ct);
        if (row is null || row.UserId != userId)
        {
            return new DeviceLinkPollResult(DeviceLinkPollState.NotFound);
        }
        var now = DateTimeOffset.UtcNow;
        if (row.CancelledAt.HasValue)
        {
            return new DeviceLinkPollResult(DeviceLinkPollState.Cancelled);
        }
        if (!row.ApprovedAt.HasValue)
        {
            return new DeviceLinkPollResult(
                now >= row.ExpiresAt ? DeviceLinkPollState.Expired : DeviceLinkPollState.Pending);
        }
        if (row.AcknowledgedAt.HasValue)
        {
            return new DeviceLinkPollResult(DeviceLinkPollState.NotFound);
        }
        if (!row.IsApprovedResultAvailable(now))
        {
            return new DeviceLinkPollResult(DeviceLinkPollState.Expired);
        }
        return new DeviceLinkPollResult(
            DeviceLinkPollState.Approved,
            row.DeviceListGeneration);
    }

    private async Task<UserIdentityBundleResponse> LoadApprovedBundleAsync(
        UserId userId,
        long? generation,
        CancellationToken ct)
    {
        if (generation is null)
        {
            throw new ConflictException("Approved link result is missing its device-list generation.");
        }
        var list = await listRepository.GetGenerationAsync(userId, generation.Value, ct)
            ?? throw new ConflictException("Approved link device-list generation is unavailable.");
        var proof = list.ParseBody();
        var listed = proof.Entries
            .Select(entry => entry.DeviceId)
            .ToHashSet(StringComparer.Ordinal);
        var allDevices = await deviceRepository.GetByUserIdAsync(userId, ct);
        var devices = allDevices.Where(device => listed.Contains(device.DeviceId)).ToList();
        if (devices.Count != listed.Count || devices.Any(device => !HasCertificateProof(device)))
        {
            throw new ConflictException("Approved link certificate proof is unavailable.");
        }
        var historical = allDevices
            .Where(device =>
                !listed.Contains(device.DeviceId)
                && device.RevokedAt.HasValue
                && HasCertificateProof(device))
            .OrderByDescending(device => device.RevokedAt)
            .ToList();
        var user = await userRepository.GetByIdAsync(userId, ct)
            ?? throw new ConflictException("Approved link identity owner is unavailable.");
        if (user.IdentityRevision <= 0 || user.IdentityIncarnationId is null)
        {
            throw new ConflictException("Approved link lifecycle proof is unavailable.");
        }
        return new UserIdentityBundleResponse(
            userId.Value,
            user.IdentityRevision,
            user.IdentityIncarnationId.Value,
            new UserDeviceListResponse(
                Convert.ToBase64String(list.Body),
                Convert.ToBase64String(list.Signature)),
            devices.Select(ToCertificateResponse).ToList(),
            historical.Select(ToCertificateResponse).ToList());
    }

    private static bool HasCertificateProof(UserDevice device)
    {
        device.RequireCertificateIntegrity();
        return true;
    }

    private static UserDeviceCertificateResponse ToCertificateResponse(UserDevice device) => new(
        Convert.ToBase64String(device.DeviceCertificate),
        Convert.ToBase64String(device.DeviceCertificateSignature));
}
