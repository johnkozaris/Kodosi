using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class DeviceEnrollmentService(
    IUserDeviceRepository devices,
    IUserDeviceListRepository deviceLists,
    IDeviceLinkRequestRepository linkRequests,
    IUserRepository users,
    PopChallengeConsumer popChallenges,
    IDeviceEnrollmentVerifier enrollmentVerifier,
    ISignedDeviceListVerifier signedListVerifier,
    IUserLifecycleLock userLifecycleLock,
    IUnitOfWork unitOfWork)
{
    private const string ChallengeConsumedSavepoint = "challenge_consumed";
    private const string MetricsEndpointTag = "device_registration";

    public async Task<DeviceEnrollmentResult> EnrollAsync(
        DeviceEnrollmentCommand command,
        CancellationToken ct = default)
    {
        await using var transaction = await unitOfWork.BeginTransactionAsync(ct);
        await userLifecycleLock.AcquireAsync(command.UserId, ct);
        if (await ResolveCommittedReplayAsync(command, ct) is { } replay)
        {
            await transaction.CommitAsync(CancellationToken.None);
            return replay;
        }

        try
        {
            await popChallenges.ConsumeAsync(
                command.UserId,
                command.ChallengeId,
                command.PopSignature,
                command.SigningPublicKey,
                MetricsEndpointTag,
                ct);
        }
        catch
        {
            await transaction.CommitAsync(CancellationToken.None);
            throw;
        }

        await transaction.CreateSavepointAsync(
            ChallengeConsumedSavepoint,
            CancellationToken.None);
        DeviceEnrollmentResult result;
        try
        {
            var currentList = await deviceLists.GetLatestAsync(command.UserId, ct);
            var verified = await enrollmentVerifier.VerifyAsync(
                command.UserId,
                command.DeviceId,
                command.KemPublicKey,
                command.SigningPublicKey,
                command.DeviceCertificate,
                command.DeviceCertificateSignature,
                command.SignedDeviceList,
                command.SignedDeviceListSignature,
                ct);
            if (await devices.GetByDeviceIdAsync(command.DeviceId, ct) is not null)
            {
                throw new DeviceAlreadyEnrolledException();
            }

            var device = UserDevice.CreateCertified(
                command.UserId,
                command.DeviceId,
                command.DeviceCertificate,
                command.DeviceCertificateSignature,
                verified.AuthorizedAt);

            var parsedList = verified.DeviceList;
            var deviceList = UserDeviceList.Create(
                command.UserId,
                parsedList.Generation,
                command.SignedDeviceList,
                command.SignedDeviceListSignature);

            await devices.AddAsync(device, ct);
            await deviceLists.AddAsync(deviceList, ct);
            await linkRequests.InvalidateApprovedBeforeGenerationAsync(
                command.UserId,
                parsedList.Generation,
                null,
                DateTimeOffset.UtcNow,
                ct);
            var user = await users.GetByIdAsync(command.UserId, ct)
                ?? throw new NotFoundException("User", command.UserId.Value);
            var bootstrapping = IsBootstrapLifecycle(user, currentList);
            Guid? incarnationId = null;
            var identityRevision = user.IdentityRevision;
            if (bootstrapping)
            {
                incarnationId = Guid.CreateVersion7();
                identityRevision = user.AdvanceIdentityLifecycle(incarnationId);
            }

            await unitOfWork.SaveChangesAsync(ct);
            result = new DeviceEnrollmentResult(
                device.DeviceId,
                deviceList.Generation,
                bootstrapping,
                identityRevision,
                incarnationId);
        }
        catch
        {
            await transaction.RollbackToSavepointAsync(
                ChallengeConsumedSavepoint,
                CancellationToken.None);
            await transaction.CommitAsync(CancellationToken.None);
            throw;
        }

        await transaction.CommitAsync(CancellationToken.None);
        return result;
    }

    private async Task<DeviceEnrollmentResult?> ResolveCommittedReplayAsync(
        DeviceEnrollmentCommand command,
        CancellationToken ct)
    {
        var device = await devices.GetByDeviceIdAsync(command.DeviceId, ct);
        if (device is null)
        {
            return null;
        }
        var parsedList = signedListVerifier.Parse(command.SignedDeviceList);
        if (!Guid.TryParse(parsedList.UserId, out var parsedUserId)
            || parsedUserId != command.UserId.Value)
        {
            return null;
        }
        var committedList = await deviceLists.GetGenerationAsync(
            command.UserId,
            parsedList.Generation,
            ct);
        if (!device.MatchesEnrollment(
                command.UserId,
                command.DeviceId,
                command.KemPublicKey,
                command.SigningPublicKey,
                command.DeviceCertificate,
                command.DeviceCertificateSignature)
            || committedList is null
            || !committedList.MatchesEnrollment(
                command.UserId,
                command.SignedDeviceList,
                command.SignedDeviceListSignature))
        {
            return null;
        }
        var latestList = await deviceLists.GetLatestAsync(command.UserId, ct)
            ?? throw new DeviceEnrollmentException(
                "Committed device enrollment list is unavailable; reset identity before retrying.");
        var user = await users.GetByIdAsync(command.UserId, ct)
            ?? throw new NotFoundException("User", command.UserId.Value);
        if (user.IdentityRevision <= 0 || user.IdentityIncarnationId is null)
        {
            throw new DeviceEnrollmentException(
                "Committed device enrollment lifecycle state is inconsistent; reset identity before retrying.");
        }
        return new DeviceEnrollmentResult(
            device.DeviceId,
            latestList.Generation,
            committedList.Generation == 1,
            user.IdentityRevision,
            user.IdentityIncarnationId);
    }

    private static bool IsBootstrapLifecycle(User user, UserDeviceList? currentList)
    {
        var bootstrapping = currentList is null;
        if ((bootstrapping && user.IdentityIncarnationId is not null)
            || (!bootstrapping
                && (user.IdentityRevision <= 0 || user.IdentityIncarnationId is null)))
        {
            throw new DeviceEnrollmentException(
                "Device enrollment lifecycle state is inconsistent; reset identity before retrying.");
        }
        return bootstrapping;
    }
}

public sealed record DeviceEnrollmentCommand(
    UserId UserId,
    string DeviceId,
    byte[] KemPublicKey,
    byte[] SigningPublicKey,
    Guid ChallengeId,
    byte[] PopSignature,
    byte[] DeviceCertificate,
    byte[] DeviceCertificateSignature,
    byte[] SignedDeviceList,
    byte[] SignedDeviceListSignature);

public sealed record DeviceEnrollmentResult(
    string DeviceId,
    long DeviceListGeneration,
    bool BootstrappedIdentity,
    long IdentityRevision,
    Guid? IdentityIncarnationId);
