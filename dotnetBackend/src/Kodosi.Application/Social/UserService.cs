using Microsoft.Extensions.Logging;
using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class UserService(
    IUserRepository users,
    IFriendshipRepository friendships,
    IUnitOfWork unitOfWork,
    IFriendRequestThrottle friendRequestThrottle,
    ILogger<UserService> logger)
{
    private readonly IUserRepository _users = users;
    private readonly IFriendshipRepository _friendships = friendships;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;
    private readonly IFriendRequestThrottle _friendRequestThrottle = friendRequestThrottle;
    private readonly ILogger<UserService> _logger = logger;

    public async Task<User?> GetByIdAsync(UserId id, CancellationToken ct = default)
        => await _users.GetByIdAsync(id, ct);

    public async Task<IReadOnlyList<User>> GetFriendsAsync(UserId userId, CancellationToken ct = default)
    {
        var friendIds = await _friendships.GetFriendIdsAsync(userId, ct);
        if (friendIds.Count == 0) return [];
        return await _users.GetByIdsAsync(friendIds, ct);
    }

    public async Task<FriendRequestInboxResponse> GetFriendRequestsAsync(
        UserId userId,
        CancellationToken ct = default)
    {
        var incoming = await _friendships.GetPendingIncomingAsync(userId, ct);
        var outgoing = await _friendships.GetPendingOutgoingAsync(userId, ct);

        var counterpartyIds = incoming.Concat(outgoing)
            .Select(friendship => friendship.GetOtherUserId(userId))
            .Distinct()
            .ToList();

        var counterpartyUsers = counterpartyIds.Count == 0
            ? []
            : await _users.GetByIdsAsync(counterpartyIds, ct);
        var usersById = counterpartyUsers.ToDictionary(user => user.Id);

        return new FriendRequestInboxResponse(
            incoming.Select(friendship => MapFriendRequest(friendship, userId, usersById)).ToList(),
            outgoing.Select(friendship => MapFriendRequest(friendship, userId, usersById)).ToList());
    }

    internal async Task<UserId> SendFriendRequestAsync(
        UserId requestorId,
        string targetHandle,
        CancellationToken ct = default)
    {
        var targetUser = await GetUserByHandleAsync(targetHandle, ct);
        if (targetUser.Id == requestorId)
        {
            throw new ConflictException("Cannot send a friend request to yourself.");
        }

        var existing = await _friendships.GetAsync(requestorId, targetUser.Id, ct);
        if (existing is not null)
        {
            throw existing.Status switch
            {
                FriendshipStatus.Accepted => new ConflictException(
                    $"You are already friends with '{targetUser.Handle}'."),
                FriendshipStatus.Pending when existing.IsRequestedBy(requestorId) => new ConflictException(
                    $"A friend request to '{targetUser.Handle}' is already pending."),
                FriendshipStatus.Pending => new ConflictException(
                    $"'{targetUser.Handle}' already sent you a friend request. Accept it instead."),
                _ => new ConflictException("A friendship already exists for that user pair."),
            };
        }

        var retryAfter = _friendRequestThrottle.TryClaim(requestorId, targetUser.Id);
        if (retryAfter is not null)
        {
            throw new FriendRequestThrottledException(retryAfter.Value);
        }

        try
        {
            await _friendships.AddAsync(Friendship.CreateRequest(requestorId, targetUser.Id), ct);
            await _unitOfWork.SaveChangesAsync(ct);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {

            _logger.LogWarning(
                ex,
                "Friend-request throttle released after unexpected exception; slot freed for {RequestorId}->{TargetId}.",
                requestorId.Value,
                targetUser.Id.Value);
            _friendRequestThrottle.Release(requestorId, targetUser.Id);
            throw;
        }

        return targetUser.Id;
    }

    internal async Task<UserId> AcceptFriendRequestAsync(
        UserId userId,
        string requestorHandle,
        CancellationToken ct = default)
    {
        var requestorUser = await GetUserByHandleAsync(requestorHandle, ct);
        var friendship = await _friendships.GetAsync(userId, requestorUser.Id, ct)
            ?? throw new NotFoundException("Friend request", requestorUser.Handle);

        if (friendship.Status != FriendshipStatus.Pending || !friendship.IsRequestedBy(requestorUser.Id))
        {
            throw new InvalidStateException(
                $"No incoming friend request from '{requestorUser.Handle}'.");
        }

        friendship.Accept(userId);
        await _unitOfWork.SaveChangesAsync(ct);
        return requestorUser.Id;
    }

    internal async Task<UserId> RejectFriendRequestAsync(
        UserId userId,
        string otherHandle,
        CancellationToken ct = default)
    {
        var otherUser = await GetUserByHandleAsync(otherHandle, ct);
        var friendship = await _friendships.GetAsync(userId, otherUser.Id, ct)
            ?? throw new NotFoundException("Friend request", otherUser.Handle);

        if (friendship.Status != FriendshipStatus.Pending || !friendship.IsRequestedBy(otherUser.Id))
        {
            throw new InvalidStateException(
                $"No incoming friend request from '{otherUser.Handle}'.");
        }

        _friendships.Remove(friendship);
        await _unitOfWork.SaveChangesAsync(ct);
        return otherUser.Id;
    }

    internal async Task<UserId> CancelOutgoingFriendRequestAsync(
        UserId userId,
        string otherHandle,
        CancellationToken ct = default)
    {
        var otherUser = await GetUserByHandleAsync(otherHandle, ct);
        if (otherUser.Id == userId)
        {
            throw new ConflictException("Cannot cancel a friend request to yourself.");
        }

        var friendship = await _friendships.GetAsync(userId, otherUser.Id, ct)
            ?? throw new NotFoundException("Friend request", otherUser.Handle);

        if (friendship.Status != FriendshipStatus.Pending || !friendship.IsRequestedBy(userId))
        {
            throw new InvalidStateException(
                $"No outgoing friend request to '{otherUser.Handle}'.");
        }

        _friendships.Remove(friendship);


        _friendRequestThrottle.Release(userId, otherUser.Id);
        await _unitOfWork.SaveChangesAsync(ct);
        return otherUser.Id;
    }

    internal async Task<FriendshipRemovalAuthorization>
        AuthorizeFriendshipRemovalAsync(
            UserId userId,
            string otherHandle,
            CancellationToken ct = default)
    {
        var otherUser = await GetUserByHandleAsync(otherHandle, ct);
        if (otherUser.Id == userId)
        {
            throw new ConflictException("Cannot unfriend yourself.");
        }

        if (await _friendships.AreFriendsAsync(userId, otherUser.Id, ct))
        {
            return new FriendshipRemovalAuthorization(
                otherUser.Id,
                otherUser.Handle);
        }

        _ = await _friendships.GetAsync(userId, otherUser.Id, ct)
            ?? throw new NotFoundException("Friendship", otherUser.Handle);

        throw new InvalidStateException(
            $"No active friendship with '{otherUser.Handle}'.");
    }

    internal async Task RemoveFriendshipAsync(
        UserId userId,
        UserId otherUserId,
        string otherHandle,
        CancellationToken ct = default)
    {
        if (otherUserId == userId)
        {
            throw new ConflictException("Cannot unfriend yourself.");
        }

        var friendship = await _friendships.GetAsync(
            userId,
            otherUserId,
            ct)
            ?? throw new NotFoundException("Friendship", otherHandle);

        if (friendship.Status != FriendshipStatus.Accepted)
        {
            throw new InvalidStateException(
                $"No active friendship with '{otherHandle}'.");
        }

        _friendships.Remove(friendship);
        await _unitOfWork.SaveChangesAsync(ct);
    }

    private async Task<User> GetUserByHandleAsync(string handle, CancellationToken ct)
    {
        var normalizedHandle = handle.Trim().ToLowerInvariant();
        var user = await _users.GetByHandleAsync(normalizedHandle, ct);
        if (user is null)
        {
            throw new NotFoundException("User", normalizedHandle);
        }

        return user;
    }

    private static FriendRequestResponse MapFriendRequest(
        Friendship friendship,
        UserId currentUserId,
        IReadOnlyDictionary<UserId, User> usersById)
    {
        var otherUserId = friendship.GetOtherUserId(currentUserId);
        if (!usersById.TryGetValue(otherUserId, out var otherUser))
        {
            throw new NotFoundException("User", otherUserId);
        }

        return new FriendRequestResponse(
            otherUser.Id.Value,
            otherUser.Handle,
            otherUser.DisplayName,
            otherUser.AvatarUrl,
            friendship.CreatedAt);
    }

}

internal sealed record FriendshipRemovalAuthorization(
    UserId OtherUserId,
    string OtherHandle);
