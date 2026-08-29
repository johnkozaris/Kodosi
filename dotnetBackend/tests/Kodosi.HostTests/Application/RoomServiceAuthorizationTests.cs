using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class RoomServiceAuthorizationTests
{
    [Fact]
    public async Task RoomMembers_Returns_Profile_Summaries_For_Members()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var room = TestRoom(ownerId);
        var rosterTransitions = new RecordingRoomRosterTransitionRepository();
        var service = RoomServiceFor(
            room,
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository(
                (room.Id, ownerId, RoomRole.Owner),
                (room.Id, actorId, RoomRole.Member)),
            new FakeUserRepository(
                User.Create(ownerId, "owner@example.com", "owner", "Owner"),
                User.Create(actorId, "actor@example.com", "actor", "Actor")),
            rosterTransitions,
            new FakeUnitOfWork());

        var result = await service.GetMembersAsync(room.Id, actorId, TestContext.Current.CancellationToken);

        Assert.NotNull(result);
        var members = Assert.IsAssignableFrom<IReadOnlyList<RoomMemberSummaryResponse>>(result);
        Assert.Collection(
            members.OrderBy(member => member.Username),
            member =>
            {
                Assert.Equal(room.Id.Value, member.RoomId);
                Assert.Equal(actorId.Value, member.UserId);
                Assert.Equal(RoomRole.Member, member.Role);
                Assert.Equal("actor", member.Username);
                Assert.Equal("Actor", member.DisplayName);
            },
            member =>
            {
                Assert.Equal(room.Id.Value, member.RoomId);
                Assert.Equal(ownerId.Value, member.UserId);
                Assert.Equal(RoomRole.Owner, member.Role);
                Assert.Equal("owner", member.Username);
                Assert.Equal("Owner", member.DisplayName);
            });
        Assert.Empty(rosterTransitions.Added);
    }

    [Fact]
    public async Task RoomMembers_Return_Null_For_NonMember()
    {
        var ownerId = UserId.New();
        var actorId = UserId.New();
        var room = TestRoom(ownerId);
        var rosterTransitions = new RecordingRoomRosterTransitionRepository();
        var service = RoomServiceFor(
            room,
            new FakeRoomRepository(room),
            new FakeRoomMemberRepository((room.Id, ownerId)),
            new FakeUserRepository(User.Create(ownerId, "owner@example.com", "owner", "Owner")),
            rosterTransitions,
            new FakeUnitOfWork());

        var result = await service.GetMembersAsync(room.Id, actorId, TestContext.Current.CancellationToken);

        Assert.Null(result);
        Assert.Empty(rosterTransitions.Added);
    }

    private static Room TestRoom(UserId ownerId) =>
        Room.Create(
            RoomId.From(Guid.NewGuid()),
            ownerId,
            "Platform",
            "platform",
            1,
            [1],
            [2],
            "owner-device");

    private static RoomService RoomServiceFor(
        Room room,
        IRoomRepository rooms,
        IRoomMemberRepository members,
        IUserRepository users,
        IRoomRosterTransitionRepository rosterTransitions,
        IUnitOfWork unitOfWork)
    {
        var device = TestDeviceCertificate.CreateDevice(
            room.OwnerUserId,
            "owner-device",
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Owner device",
            signerDeviceId: "owner-device",
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: null);
        var list = TestDeviceList.Create(
            room.OwnerUserId,
            1,
            """[{"deviceId":"owner-device","signerDeviceId":"owner-device"}]""",
            "owner-device",
            [2],
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            null);
        return new RoomService(
            rooms,
            members,
            users,
            new RoomRosterVerifier(
                new FakeUserDeviceRepository(device),
                new FakeUserDeviceListRepository(list),
                new AlwaysValidSignatureVerifier(),
                TimeProvider.System),
            rosterTransitions,
            unitOfWork,
            new FakeRoomLifecycleLock());
    }
}
