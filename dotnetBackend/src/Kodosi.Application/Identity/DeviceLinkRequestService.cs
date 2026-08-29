using System.Security.Cryptography;
using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class DeviceLinkRequestService(
    IDeviceLinkRequestRepository links,
    IUserLifecycleLock userLifecycleLock,
    IUnitOfWork unitOfWork,
    TimeProvider timeProvider)
{
    private const int DeviceCodeByteLength = 32;
    private const int UserCodeGroupLength = 4;
    private const int UserCodeGroups = 2;
    private const int UserCodeCollisionRetries = 5;
    private const int MaxPendingRequestsPerUser = 3;
    private const string UserCodeAlphabet = "BCDFGHJKMNPQRSTVWXZ23456789";
    private static readonly TimeSpan LinkRequestTtl = TimeSpan.FromMinutes(15);

    public async Task<DeviceLinkInitiationResult> InitiateAsync(
        UserId userId,
        string deviceId,
        string deviceLabel,
        byte[] kemPublicKey,
        byte[] signingPublicKey,
        CancellationToken ct = default)
    {
        if (string.IsNullOrWhiteSpace(deviceId))
        {
            throw new DeviceEnrollmentException("deviceId is required.");
        }

        await using var transaction = await unitOfWork.BeginTransactionAsync(ct);
        await userLifecycleLock.AcquireAsync(userId, ct);
        var now = timeProvider.GetUtcNow();
        var pending = await links.ListPendingForUserAsync(userId, now, ct);
        var replay = pending.SingleOrDefault(request => request.MatchesInitiation(
            deviceId,
            deviceLabel,
            kemPublicKey,
            signingPublicKey));
        if (replay is not null)
        {
            await transaction.CommitAsync(CancellationToken.None);
            return new DeviceLinkInitiationResult(
                replay.DeviceCode,
                replay.UserCode,
                replay.DeviceLabel,
                replay.ExpiresAt);
        }

        var pendingCount = pending.Count;
        if (pendingCount >= MaxPendingRequestsPerUser)
        {
            throw new DeviceEnrollmentException(
                $"You already have {pendingCount} pending link requests; approve or cancel one before initiating another.");
        }

        const string AllocationSavepoint = "device_link_code_allocation";
        for (var attempt = 0; attempt < UserCodeCollisionRetries; attempt++)
        {
            await transaction.CreateSavepointAsync(AllocationSavepoint, ct);
            var deviceCode = GenerateDeviceCode();
            var userCode = await AllocateUserCodeAsync(ct);
            var request = DeviceLinkRequest.Create(
                userId,
                deviceCode,
                userCode,
                deviceId,
                deviceLabel,
                kemPublicKey,
                signingPublicKey,
                LinkRequestTtl,
                now);
            await links.AddAsync(request, ct);
            try
            {
                await unitOfWork.SaveChangesAsync(ct);
                await transaction.CommitAsync(CancellationToken.None);
                return new DeviceLinkInitiationResult(
                    request.DeviceCode,
                    request.UserCode,
                    request.DeviceLabel,
                    request.ExpiresAt);
            }
            catch (DeviceLinkUserCodeCollisionException exception)
            {
                await transaction.RollbackToSavepointAsync(AllocationSavepoint, ct);
                if (attempt + 1 == UserCodeCollisionRetries)
                {
                    throw new ConflictException(
                        "Could not allocate a unique user_code; retry shortly.",
                        exception);
                }
            }
        }

        throw new ConflictException("Could not allocate a unique user_code; retry shortly.");
    }

    public async Task<DeviceLinkPendingResult?> GetPendingAsync(
        UserId userId,
        string? userCode,
        CancellationToken ct = default)
    {
        if (string.IsNullOrWhiteSpace(userCode))
        {
            return null;
        }

        var normalized = NormalizeUserCode(userCode);
        var request = await links.GetByUserCodeAsync(normalized, ct);
        if (request is null
            || request.UserId != userId
            || !request.IsPending(timeProvider.GetUtcNow()))
        {
            return null;
        }
        return new DeviceLinkPendingResult(
            request.DeviceId,
            request.DeviceLabel,
            request.KemPublicKey,
            request.SigningPublicKey);
    }

    public async Task<DeviceLinkAcknowledgeResult> AcknowledgeAsync(
        UserId userId,
        string? deviceCode,
        string? deviceId,
        CancellationToken ct = default)
    {
        if (string.IsNullOrWhiteSpace(deviceCode)
            || string.IsNullOrWhiteSpace(deviceId))
        {
            return DeviceLinkAcknowledgeResult.NotFound;
        }

        var request = await links.GetByDeviceCodeAsync(deviceCode, ct);
        if (request is null
            || request.UserId != userId
            || !string.Equals(request.DeviceId, deviceId, StringComparison.Ordinal))
        {
            return DeviceLinkAcknowledgeResult.NotFound;
        }

        var outcome = await links.AcknowledgeApprovedAsync(
            request.Id,
            userId,
            deviceId,
            timeProvider.GetUtcNow(),
            ct);
        return outcome switch
        {
            DeviceLinkAcknowledgeOutcome.Acknowledged
                or DeviceLinkAcknowledgeOutcome.AlreadyAcknowledged =>
                DeviceLinkAcknowledgeResult.Acknowledged,
            DeviceLinkAcknowledgeOutcome.Invalidated =>
                throw new DeviceLinkReceiptInvalidatedException(),
            _ => DeviceLinkAcknowledgeResult.NotFound,
        };
    }

    public async Task<DeviceLinkCancellationResult?> CancelAsync(
        UserId userId,
        string? userCode,
        CancellationToken ct = default)
    {
        if (string.IsNullOrWhiteSpace(userCode))
        {
            return null;
        }

        var normalized = NormalizeUserCode(userCode);
        await using var transaction = await unitOfWork.BeginTransactionAsync(ct);
        await userLifecycleLock.AcquireAsync(userId, ct);
        var outcome = await links.CancelPendingAsync(
            normalized,
            userId,
            timeProvider.GetUtcNow(),
            ct);
        await transaction.CommitAsync(CancellationToken.None);
        return outcome switch
        {
            DeviceLinkCancelOutcome.Cancelled
                or DeviceLinkCancelOutcome.AlreadyCancelled =>
                new DeviceLinkCancellationResult(normalized),
            DeviceLinkCancelOutcome.Approved =>
                throw new ConflictException("Cannot cancel a request that has already been approved."),
            DeviceLinkCancelOutcome.Expired =>
                throw new ConflictException("Cannot cancel a request that has expired."),
            _ => null,
        };
    }

    private async Task<string> AllocateUserCodeAsync(CancellationToken ct)
    {
        for (var attempt = 0; attempt < UserCodeCollisionRetries; attempt++)
        {
            var candidate = GenerateUserCode();
            if (await links.IsUserCodeAvailableAsync(candidate, ct))
            {
                return candidate;
            }
        }
        throw new ConflictException("Could not allocate a unique user_code; retry shortly.");
    }

    private static string NormalizeUserCode(string userCode) =>
        userCode.Trim().ToUpperInvariant();

    private static string GenerateUserCode()
    {
        var chars = new char[UserCodeGroupLength * UserCodeGroups + UserCodeGroups - 1];
        var index = 0;
        for (var group = 0; group < UserCodeGroups; group++)
        {
            if (group > 0)
            {
                chars[index++] = '-';
            }
            for (var i = 0; i < UserCodeGroupLength; i++)
            {
                chars[index++] = UserCodeAlphabet[
                    RandomNumberGenerator.GetInt32(0, UserCodeAlphabet.Length)];
            }
        }
        return new string(chars);
    }

    private static string GenerateDeviceCode() =>
        Convert.ToBase64String(RandomNumberGenerator.GetBytes(DeviceCodeByteLength))
            .Replace('+', '-')
            .Replace('/', '_')
            .TrimEnd('=');
}

public sealed record DeviceLinkInitiationResult(
    string DeviceCode,
    string UserCode,
    string DeviceLabel,
    DateTimeOffset ExpiresAt);

public sealed record DeviceLinkPendingResult(
    string DeviceId,
    string DeviceLabel,
    byte[] KemPublicKey,
    byte[] SigningPublicKey);

public enum DeviceLinkAcknowledgeResult
{
    NotFound,
    Acknowledged,
}

public sealed record DeviceLinkCancellationResult(string UserCode);
