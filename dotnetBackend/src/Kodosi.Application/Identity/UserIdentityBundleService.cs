using Kodosi.Domain;

namespace Kodosi.Application;


public sealed class UserIdentityBundleService(
    IUserDeviceRepository deviceRepository,
    IUserDeviceListRepository listRepository,
    IUserRepository userRepository,
    IFriendshipRepository friendshipRepository,
    ISharedRoomAuthorizationRepository sharedRoomAuthorization,
    IAccessOverrideRepository accessOverrideRepository,
    IIdentityExposureRepository identityExposureRepository,
    IUserLifecycleLock userLifecycleLock,
    IRoomLifecycleLock roomLifecycleLock,
    IUnitOfWork unitOfWork)
{
    private readonly IUserDeviceRepository _deviceRepository = deviceRepository;
    private readonly IUserDeviceListRepository _listRepository = listRepository;
    private readonly IUserRepository _userRepository = userRepository;
    private readonly IFriendshipRepository _friendshipRepository = friendshipRepository;
    private readonly ISharedRoomAuthorizationRepository _sharedRoomAuthorization =
        sharedRoomAuthorization;
    private readonly IAccessOverrideRepository _accessOverrideRepository = accessOverrideRepository;
    private readonly IIdentityExposureRepository _identityExposureRepository =
        identityExposureRepository;
    private readonly IUserLifecycleLock _userLifecycleLock = userLifecycleLock;
    private readonly IRoomLifecycleLock _roomLifecycleLock = roomLifecycleLock;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;

    public async Task<UserIdentityBundle?> GetAsync(
        UserId callerUserId,
        UserId targetUserId,
        CancellationToken ct = default)
    {
        var isSelfRead = callerUserId == targetUserId;
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _userLifecycleLock.AcquireAsync(targetUserId, ct);

        var list = await _listRepository.GetLatestAsync(targetUserId, ct).ConfigureAwait(false);
        var user = await _userRepository.GetByIdAsync(targetUserId, ct).ConfigureAwait(false);
        var devices = await _deviceRepository.GetByUserIdAsync(targetUserId, ct).ConfigureAwait(false);

        var areFriends = false;
        var shareRoom = false;
        var hasOverride = false;
        if (!isSelfRead)
        {
            areFriends = await _friendshipRepository
                .AreFriendsAsync(callerUserId, targetUserId, ct)
                .ConfigureAwait(false);
            var candidateRoomIds = await _sharedRoomAuthorization
                .GetSharedActiveRoomIdsAsync(callerUserId, targetUserId, ct)
                .ConfigureAwait(false);
            foreach (var roomId in candidateRoomIds.OrderBy(roomId => roomId.Value))
            {
                await _roomLifecycleLock.AcquireAsync(roomId, ct).ConfigureAwait(false);
            }
            shareRoom = candidateRoomIds.Count > 0
                && (await _sharedRoomAuthorization
                    .GetSharedActiveRoomIdsAsync(callerUserId, targetUserId, ct)
                    .ConfigureAwait(false)).Count > 0;
            hasOverride = await _accessOverrideRepository
                .HasActiveRelationshipAsync(callerUserId, targetUserId, ct)
                .ConfigureAwait(false);
        }

        if (list is null
            || user?.IdentityIncarnationId is null
            || user.IdentityRevision <= 0)
        {
            return null;
        }
        var now = DateTimeOffset.UtcNow;
        if (!ActiveDeviceAuthorization.IsCurrent(list, targetUserId, now))
        {
            return null;
        }

        if (!isSelfRead && !areFriends && !shareRoom && !hasOverride)
        {
            return null;
        }

        var listedDeviceIds = list.ParseBody().Entries
            .Select(entry => entry.DeviceId)
            .ToHashSet(StringComparer.Ordinal);
        var certified = devices
            .Where(d => ActiveDeviceAuthorization.IsCertificateActive(
                d,
                targetUserId,
                now))
            .Where(d => listedDeviceIds.Contains(d.DeviceId))
            .ToList();
        var historical = devices
            .Where(device =>
                device.UserId == targetUserId
                && device.RevokedAt.HasValue)
            .OrderByDescending(device => device.RevokedAt)
            .ToList();
        foreach (var device in historical)
        {
            device.RequireCertificateIntegrity();
        }


        if (!isSelfRead && certified.Count == 0)
        {
            return null;
        }

        if (!isSelfRead)
        {
            await _identityExposureRepository.RecordAsync(
                targetUserId,
                callerUserId,
                DateTimeOffset.UtcNow,
                ct);
            await _unitOfWork.SaveChangesAsync(ct);
        }
        await transaction.CommitAsync(ct);
        return new UserIdentityBundle(
            targetUserId,
            user.IdentityRevision,
            user.IdentityIncarnationId.Value,
            list,
            certified,
            historical);
    }
}

public sealed record UserIdentityBundle(
    UserId UserId,
    long IdentityRevision,
    Guid IdentityIncarnationId,
    UserDeviceList DeviceList,
    IReadOnlyList<UserDevice> Devices,
    IReadOnlyList<UserDevice> HistoricalDevices);
