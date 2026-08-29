using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class RoomService(
    IRoomRepository rooms,
    IRoomMemberRepository members,
    IUserRepository users,
    RoomRosterVerifier rosterVerifier,
    IRoomRosterTransitionRepository rosterTransitions,
    IUnitOfWork unitOfWork,
    IRoomLifecycleLock roomLifecycleLock)
{
    private readonly IRoomRepository _rooms = rooms;
    private readonly IRoomMemberRepository _members = members;
    private readonly IUserRepository _users = users;
    private readonly RoomRosterVerifier _rosterVerifier = rosterVerifier;
    private readonly IRoomRosterTransitionRepository _rosterTransitions = rosterTransitions;
    private readonly IUnitOfWork _unitOfWork = unitOfWork;
    private readonly IRoomLifecycleLock _roomLifecycleLock = roomLifecycleLock;

    public async Task<Room> CreateAsync(
        RoomId roomId,
        UserId ownerUserId,
        string name,
        string slug,
        long rosterGeneration,
        byte[] rosterBody,
        byte[] rosterSignature,
        string rosterSignerDeviceId,
        CancellationToken ct = default)
    {
        await using var transaction = await _unitOfWork.BeginTransactionAsync(ct);
        await _roomLifecycleLock.AcquireAsync(roomId, ct);
        var existing = await _rooms.GetByIdAsync(roomId, ct);
        if (existing is not null)
        {
            if (!existing.IsOwner(ownerUserId)
                || existing.Name != name
                || existing.Slug != slug
                || existing.RosterGeneration != rosterGeneration
                || !existing.RosterBody.SequenceEqual(rosterBody)
                || !existing.RosterSignature.SequenceEqual(rosterSignature)
                || existing.RosterSignerDeviceId != rosterSignerDeviceId)
            {
                throw new ConflictException(
                    "Room ID is already in use with a different request fingerprint.");
            }

            await transaction.CommitAsync(ct);
            return existing;
        }

        await _rosterVerifier.VerifyAsync(
            roomId,
            ownerUserId,
            rosterGeneration,
            rosterBody,
            rosterSignature,
            rosterSignerDeviceId,
            [ownerUserId],
            ct);
        var room = Room.Create(
            roomId,
            ownerUserId,
            name,
            slug,
            rosterGeneration,
            rosterBody,
            rosterSignature,
            rosterSignerDeviceId);
        await _rooms.AddAsync(room, ct);
        await _rosterTransitions.AddAsync(RoomRosterTransition.Create(room), ct);

        var member = RoomMember.CreateOwner(room.Id, ownerUserId);
        await _members.AddAsync(member, ct);

        await _unitOfWork.SaveChangesAsync(ct);
        await transaction.CommitAsync(ct);
        return room;
    }

    public async Task<Room?> GetByIdAsync(
        RoomId id,
        UserId requestorId,
        CancellationToken ct = default)
    {
        var room = await _rooms.GetByIdAsync(id, ct);
        if (room is null)
        {
            return null;
        }

        return await _members.IsMemberAsync(id, requestorId, ct) ? room : null;
    }

    public async Task<IReadOnlyList<RoomMemberSummaryResponse>?> GetMembersAsync(
        RoomId roomId,
        UserId requestorId,
        CancellationToken ct = default)
    {
        var room = await _rooms.GetByIdAsync(roomId, ct);
        if (room is null)
        {
            return null;
        }

        if (!await _members.IsMemberAsync(roomId, requestorId, ct))
        {
            return null;
        }

        var memberships = await _members.GetActiveByRoomAsync(roomId, ct);
        if (memberships.Count == 0)
        {
            return [];
        }
        var memberIds = memberships.Select(membership => membership.UserId).Distinct().ToList();

        var usersById = (await _users.GetByIdsAsync(memberIds, ct))
            .ToDictionary(user => user.Id);

        var summaries = new List<RoomMemberSummaryResponse>(memberIds.Count);
        foreach (var membership in memberships)
        {
            if (usersById.TryGetValue(membership.UserId, out var user))
            {
                summaries.Add(new RoomMemberSummaryResponse(
                    roomId.Value,
                    user.Id.Value,
                    membership.Role,
                    user.Handle,
                    user.DisplayName,
                    user.AvatarUrl));
            }
        }

        return summaries;
    }

    internal Task<Room?> GetByIdAsync(
        RoomId roomId,
        CancellationToken ct = default) =>
        _rooms.GetByIdAsync(roomId, ct);

    internal async Task RemoveMemberWithLifecycleHeldAsync(
        RoomId roomId,
        UserId requestorId,
        UserId memberUserId,
        long rosterGeneration,
        byte[] rosterBody,
        byte[] rosterSignature,
        string rosterSignerDeviceId,
        CancellationToken ct = default)
    {
        var room = await _rooms.GetByIdAsync(roomId, ct)
            ?? throw new NotFoundException(nameof(Room), roomId);

        var isOwnerRequestor = room.IsOwner(requestorId);
        if (!isOwnerRequestor)
            throw new PolicyViolationException("Only the room owner can remove other members.");

        if (room.IsOwner(memberUserId))
            throw new PolicyViolationException("The room owner cannot remove themselves.");

        var member = await _members.GetAsync(roomId, memberUserId, ct)
            ?? throw new NotFoundException("RoomMember", memberUserId);

        if (!member.IsActive)
            throw new NotFoundException("RoomMember", memberUserId);

        await ApplyRemovalRosterAsync(
            room,
            rosterGeneration,
            rosterBody,
            rosterSignature,
            rosterSignerDeviceId,
            memberUserId,
            ct);
        member.Revoke();
        await _unitOfWork.SaveChangesAsync(ct);
    }

    private async Task ApplyRemovalRosterAsync(
        Room room,
        long generation,
        byte[] body,
        byte[] signature,
        string signerDeviceId,
        UserId excludedMember,
        CancellationToken ct)
    {
        var activeMembers = (await _members.GetMemberUserIdsAsync(room.Id, ct))
            .ToHashSet();
        activeMembers.Remove(excludedMember);
        activeMembers.Add(room.OwnerUserId);
        await _rosterVerifier.VerifyAsync(
            room.Id,
            room.OwnerUserId,
            generation,
            body,
            signature,
            signerDeviceId,
            activeMembers,
            ct);
        room.ReplaceRosterForRemoval(generation, body, signature, signerDeviceId);
        await _rosterTransitions.AddAsync(RoomRosterTransition.Create(room), ct);
    }
}
