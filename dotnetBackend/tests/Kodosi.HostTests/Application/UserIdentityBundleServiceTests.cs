using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class UserIdentityBundleServiceTests
{
    private static readonly byte[] DummyKemKey = new byte[1184];
    private static readonly byte[] DummySignKey = new byte[1952];
    private static readonly byte[] DummySignature = [0x01, 0x02, 0x03];

    [Fact]
    public async Task Returns_Bundle_For_Self_Read()
    {
        var userId = UserId.New();
        var device = CreateCertifiedDevice(userId, "device-1");
        var list = CreateDeviceList(userId, generation: 1, signerDeviceId: "device-1");

        var service = CreateService(
            devices: [device],
            lists: [list]);

        var bundle = await service.GetAsync(userId, userId, TestContext.Current.CancellationToken);

        Assert.NotNull(bundle);
        Assert.Equal(userId, bundle.UserId);
        Assert.Single(bundle.Devices);
        Assert.Equal(1, bundle.DeviceList.Generation);
    }

    [Fact]
    public async Task Rejects_Corrupt_Active_Certificate_Signature()
    {
        var userId = UserId.New();
        var device = CreateCertifiedDevice(userId, "device-1");
        DomainFixtureHydrator.SetDeviceCertificateSignature(device, [1]);
        var list = CreateDeviceList(userId, generation: 1, signerDeviceId: "device-1");
        var service = CreateService(devices: [device], lists: [list]);

        await Assert.ThrowsAsync<DeviceCertificateCorruptionException>(() =>
            service.GetAsync(userId, userId, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Successful_NonSelf_Read_Records_Durable_Exposure()
    {
        var callerId = UserId.New();
        var targetId = UserId.New();
        var device = CreateCertifiedDevice(targetId, "device-1");
        var list = CreateDeviceList(targetId, generation: 1, signerDeviceId: "device-1");
        var exposures = new RecordingIdentityExposureRepository();
        var service = new UserIdentityBundleService(
            new FakeUserDeviceRepository(device),
            new FakeUserDeviceListRepository(list),
            new FakeUserRepository(CreateLifecycleUser(targetId)),
            new FakeFriendshipRepository(areFriends: true),
            new FakeSharedRoomAuthorizationRepository(),
            new FakeAccessOverrideRepository(),
            exposures,
            new FakeUserLifecycleLock(),
            new FakeRoomLifecycleLock(),
            new FakeUnitOfWork());

        Assert.NotNull(await service.GetAsync(
            callerId,
            targetId,
            TestContext.Current.CancellationToken));

        Assert.Equal([(targetId, callerId)], exposures.Records);
    }

    [Fact]
    public async Task Room_Authorization_Is_Rechecked_After_Lifecycle_Lock()
    {
        var callerId = UserId.New();
        var targetId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var device = CreateCertifiedDevice(targetId, "device-1");
        var list = CreateDeviceList(targetId, generation: 1, signerDeviceId: "device-1");
        var rooms = new SequencedSharedRoomAuthorizationRepository([roomId], []);
        var exposures = new RecordingIdentityExposureRepository();
        var roomLock = new RecordingRoomLifecycleLock();
        var service = new UserIdentityBundleService(
            new FakeUserDeviceRepository(device),
            new FakeUserDeviceListRepository(list),
            new FakeUserRepository(CreateLifecycleUser(targetId)),
            new FakeFriendshipRepository(),
            rooms,
            new FakeAccessOverrideRepository(),
            exposures,
            new FakeUserLifecycleLock(),
            roomLock,
            new FakeUnitOfWork());

        var bundle = await service.GetAsync(
            callerId,
            targetId,
            TestContext.Current.CancellationToken);

        Assert.Null(bundle);
        Assert.Empty(exposures.Records);
        Assert.Equal([roomId], roomLock.AcquiredRoomIds);
    }

    [Fact]
    public async Task Historical_Author_Verification_Is_Directional_And_Requires_Current_Reader_Access()
    {
        var readerId = UserId.New();
        var authorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var rooms = new HistoricalArtifactAuthorizationRepository(readerId, authorId, [roomId], [roomId], []);
        var exposures = new RecordingIdentityExposureRepository();
        var locks = new RecordingRoomLifecycleLock();
        var service = new UserIdentityBundleService(
            new FakeUserDeviceRepository(CreateCertifiedDevice(authorId, "author-device"), CreateCertifiedDevice(readerId, "reader-device")),
            new FakeUserDeviceListRepository(CreateDeviceList(authorId, 1, "author-device"), CreateDeviceList(readerId, 1, "reader-device")),
            new FakeUserRepository(CreateLifecycleUser(authorId), CreateLifecycleUser(readerId)),
            new FakeFriendshipRepository(), rooms, new FakeAccessOverrideRepository(),
            exposures, new FakeUserLifecycleLock(), locks, new FakeUnitOfWork());

        Assert.NotNull(await service.GetAsync(readerId, authorId, TestContext.Current.CancellationToken));
        Assert.Equal([(authorId, readerId)], exposures.Records);
        Assert.Equal([roomId], locks.AcquiredRoomIds);
        Assert.Null(await service.GetAsync(authorId, readerId, TestContext.Current.CancellationToken));
        Assert.Null(await service.GetAsync(readerId, authorId, TestContext.Current.CancellationToken));
        Assert.Single(exposures.Records);
    }

    [Fact]
    public async Task Historical_Author_Access_Is_Rechecked_Only_Against_Locked_Rooms()
    {
        var readerId = UserId.New();
        var authorId = UserId.New();
        var roomId = RoomId.From(Guid.NewGuid());
        var unlockedRoomId = RoomId.From(Guid.NewGuid());
        var rooms = new HistoricalArtifactAuthorizationRepository(readerId, authorId, [roomId], [unlockedRoomId]);
        var exposures = new RecordingIdentityExposureRepository();
        var locks = new RecordingRoomLifecycleLock();
        var service = new UserIdentityBundleService(
            new FakeUserDeviceRepository(CreateCertifiedDevice(authorId, "device-1")),
            new FakeUserDeviceListRepository(CreateDeviceList(authorId, 1, "device-1")),
            new FakeUserRepository(CreateLifecycleUser(authorId)),
            new FakeFriendshipRepository(), rooms, new FakeAccessOverrideRepository(),
            exposures, new FakeUserLifecycleLock(), locks, new FakeUnitOfWork());

        Assert.Null(await service.GetAsync(readerId, authorId, TestContext.Current.CancellationToken));
        Assert.Empty(exposures.Records);
        Assert.Equal([roomId], locks.AcquiredRoomIds);
    }

    [Fact]
    public async Task Returns_Bundle_When_Caller_And_Target_Are_Friends()
    {
        var callerId = UserId.New();
        var targetId = UserId.New();
        var device = CreateCertifiedDevice(targetId, "device-1");
        var list = CreateDeviceList(targetId, generation: 1, signerDeviceId: "device-1");

        var service = CreateService(
            devices: [device],
            lists: [list],
            friendships: new FakeFriendshipRepository(areFriends: true));

        var bundle = await service.GetAsync(callerId, targetId, TestContext.Current.CancellationToken);

        Assert.NotNull(bundle);
        Assert.Equal(targetId, bundle.UserId);
    }

    [Fact]
    public async Task Returns_Bundle_When_Caller_Has_Active_Access_Override_From_Target()
    {
        var callerId = UserId.New();
        var targetId = UserId.New();
        var sessionId = SessionId.New();
        var accessOverride = SessionAccessOverride.Create(
            sessionId, callerId, AccessLevel.View, targetId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(accessOverride)
            .WithSessionOwner(sessionId, targetId);

        var device = CreateCertifiedDevice(targetId, "device-1");
        var list = CreateDeviceList(targetId, generation: 1, signerDeviceId: "device-1");

        var service = CreateService(
            devices: [device],
            lists: [list],
            accessOverrides: overrides);

        var bundle = await service.GetAsync(callerId, targetId, TestContext.Current.CancellationToken);

        Assert.NotNull(bundle);
    }

    [Fact]
    public async Task Returns_Bundle_When_Caller_Is_Session_Owner_And_Target_Is_Grantee()
    {
        var callerId = UserId.New();
        var targetId = UserId.New();
        var sessionId = SessionId.New();
        var accessOverride = SessionAccessOverride.Create(
            sessionId, targetId, AccessLevel.View, callerId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(accessOverride)
            .WithSessionOwner(sessionId, callerId);

        var device = CreateCertifiedDevice(targetId, "device-1");
        var list = CreateDeviceList(targetId, generation: 1, signerDeviceId: "device-1");

        var service = CreateService(
            devices: [device],
            lists: [list],
            accessOverrides: overrides);

        var bundle = await service.GetAsync(callerId, targetId, TestContext.Current.CancellationToken);

        Assert.NotNull(bundle);
    }

    [Fact]
    public async Task Returns_Null_When_Caller_Has_No_Trust_Relationship()
    {
        var callerId = UserId.New();
        var targetId = UserId.New();
        var device = CreateCertifiedDevice(targetId, "device-1");
        var list = CreateDeviceList(targetId, generation: 1, signerDeviceId: "device-1");

        var service = CreateService(devices: [device], lists: [list]);

        var bundle = await service.GetAsync(callerId, targetId, TestContext.Current.CancellationToken);

        Assert.Null(bundle);
    }

    [Fact]
    public async Task Returns_Null_When_Revoked_Override_Is_The_Only_Relationship()
    {
        var callerId = UserId.New();
        var targetId = UserId.New();
        var sessionId = SessionId.New();
        var accessOverride = SessionAccessOverride.Create(
            sessionId, callerId, AccessLevel.View, targetId, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        accessOverride.Revoke();
        var overrides = new FakeAccessOverrideRepository(accessOverride)
            .WithSessionOwner(sessionId, targetId);

        var device = CreateCertifiedDevice(targetId, "device-1");
        var list = CreateDeviceList(targetId, generation: 1, signerDeviceId: "device-1");

        var service = CreateService(
            devices: [device],
            lists: [list],
            accessOverrides: overrides);

        var bundle = await service.GetAsync(callerId, targetId, TestContext.Current.CancellationToken);

        Assert.Null(bundle);
    }

    [Fact]
    public async Task Returns_Null_When_Target_Has_No_Device_List()
    {
        var userId = UserId.New();
        var service = CreateService();

        var bundle = await service.GetAsync(userId, userId, TestContext.Current.CancellationToken);

        Assert.Null(bundle);
    }

    [Fact]
    public async Task Self_Read_Returns_Bundle_Even_When_Devices_Empty()
    {
        var userId = UserId.New();
        var list = CreateDeviceList(userId, generation: 1, signerDeviceId: "device-1");

        var service = CreateService(devices: [], lists: [list]);

        var bundle = await service.GetAsync(userId, userId, TestContext.Current.CancellationToken);

        Assert.NotNull(bundle);
        Assert.Empty(bundle.Devices);
    }

    [Fact]
    public async Task Non_Self_Read_Returns_Null_When_Target_Has_No_Certified_Device()
    {
        var callerId = UserId.New();
        var targetId = UserId.New();
        var list = CreateDeviceList(targetId, generation: 1, signerDeviceId: "device-1");

        var service = CreateService(
            devices: [],
            lists: [list],
            friendships: new FakeFriendshipRepository(areFriends: true));

        var bundle = await service.GetAsync(callerId, targetId, TestContext.Current.CancellationToken);

        Assert.Null(bundle);
    }

    [Fact]
    public async Task Separates_Revoked_Devices_Into_Historical_Signer_Epochs()
    {
        var userId = UserId.New();
        var active = CreateCertifiedDevice(userId, "device-1");
        var revoked = CreateCertifiedDevice(userId, "device-2");
        revoked.Revoke("device-1");
        var list = CreateDeviceList(userId, generation: 2, signerDeviceId: "device-1");

        var service = CreateService(devices: [active, revoked], lists: [list]);

        var bundle = await service.GetAsync(userId, userId, TestContext.Current.CancellationToken);

        Assert.NotNull(bundle);
        Assert.Single(bundle.Devices);
        Assert.Equal("device-1", bundle.Devices[0].DeviceId);
        var historical = Assert.Single(bundle.HistoricalDevices);
        Assert.Equal("device-2", historical.DeviceId);
        Assert.NotNull(historical.RevokedAt);
    }

    [Fact]
    public async Task Rejects_Corrupt_Historical_Certificate()
    {
        var userId = UserId.New();
        var active = CreateCertifiedDevice(userId, "device-1");
        var revoked = CreateCertifiedDevice(userId, "device-2");
        revoked.Revoke("device-1");
        DomainFixtureHydrator.SetDeviceCertificate(revoked, [0]);
        var list = CreateDeviceList(userId, generation: 2, signerDeviceId: "device-1");
        var service = CreateService(devices: [active, revoked], lists: [list]);

        await Assert.ThrowsAsync<DeviceCertificateCorruptionException>(() =>
            service.GetAsync(userId, userId, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Override_Check_Ignores_Unrelated_Sessions()
    {
        var callerId = UserId.New();
        var targetId = UserId.New();
        var thirdUser = UserId.New();
        var sessionId = SessionId.New();
        var accessOverride = SessionAccessOverride.Create(
            sessionId, callerId, AccessLevel.View, thirdUser, DateTimeOffset.UtcNow.AddHours(24), DateTimeOffset.UtcNow);
        var overrides = new FakeAccessOverrideRepository(accessOverride)
            .WithSessionOwner(sessionId, thirdUser);

        var device = CreateCertifiedDevice(targetId, "device-1");
        var list = CreateDeviceList(targetId, generation: 1, signerDeviceId: "device-1");

        var service = CreateService(
            devices: [device],
            lists: [list],
            accessOverrides: overrides);

        var bundle = await service.GetAsync(callerId, targetId, TestContext.Current.CancellationToken);

        Assert.Null(bundle);
    }

    [Fact]
    public async Task NonSelf_Reads_Run_Identical_Trust_Lookups_Whether_Granted_Or_Denied()
    {
        var callerId = UserId.New();
        var targetId = UserId.New();
        var device = CreateCertifiedDevice(targetId, "device-1");
        var list = CreateDeviceList(targetId, generation: 1, signerDeviceId: "device-1");

        var grantedFriendships = new FakeFriendshipRepository(areFriends: true);
        var grantedOverrides = new FakeAccessOverrideRepository();
        var granted = new UserIdentityBundleService(
            new FakeUserDeviceRepository(device),
            new FakeUserDeviceListRepository(list),
            new FakeUserRepository(CreateLifecycleUser(targetId)),
            grantedFriendships,
            new FakeSharedRoomAuthorizationRepository(),
            grantedOverrides,
            new RecordingIdentityExposureRepository(),
            new FakeUserLifecycleLock(),
            new FakeRoomLifecycleLock(),
            new FakeUnitOfWork());

        var deniedFriendships = new FakeFriendshipRepository();
        var deniedOverrides = new FakeAccessOverrideRepository();
        var denied = new UserIdentityBundleService(
            new FakeUserDeviceRepository(device),
            new FakeUserDeviceListRepository(list),
            new FakeUserRepository(CreateLifecycleUser(targetId)),
            deniedFriendships,
            new FakeSharedRoomAuthorizationRepository(),
            deniedOverrides,
            new RecordingIdentityExposureRepository(),
            new FakeUserLifecycleLock(),
            new FakeRoomLifecycleLock(),
            new FakeUnitOfWork());

        Assert.NotNull(await granted.GetAsync(
            callerId, targetId, TestContext.Current.CancellationToken));
        Assert.Null(await denied.GetAsync(
            callerId, targetId, TestContext.Current.CancellationToken));

        Assert.Equal(1, grantedFriendships.AreFriendsCalls);
        Assert.Equal(1, grantedOverrides.HasActiveRelationshipCalls);
        Assert.Equal(grantedFriendships.AreFriendsCalls, deniedFriendships.AreFriendsCalls);
        Assert.Equal(
            grantedOverrides.HasActiveRelationshipCalls,
            deniedOverrides.HasActiveRelationshipCalls);
    }

    private static UserIdentityBundleService CreateService(
        UserDevice[]? devices = null,
        UserDeviceList[]? lists = null,
        FakeFriendshipRepository? friendships = null,
        FakeAccessOverrideRepository? accessOverrides = null)
    {
        var targetUserId = lists?.FirstOrDefault()?.UserId
            ?? devices?.FirstOrDefault()?.UserId
            ?? UserId.New();
        return new UserIdentityBundleService(
            new FakeUserDeviceRepository(devices ?? []),
            new FakeUserDeviceListRepository(lists ?? []),
            new FakeUserRepository(CreateLifecycleUser(targetUserId)),
            friendships ?? new FakeFriendshipRepository(),
            new FakeSharedRoomAuthorizationRepository(),
            accessOverrides ?? new FakeAccessOverrideRepository(),
            new RecordingIdentityExposureRepository(),
            new FakeUserLifecycleLock(),
            new FakeRoomLifecycleLock(),
            new FakeUnitOfWork());
    }

    private sealed class RecordingRoomLifecycleLock : IRoomLifecycleLock
    {
        public List<RoomId> AcquiredRoomIds { get; } = [];

        public Task AcquireAsync(RoomId roomId, CancellationToken ct = default)
        {
            AcquiredRoomIds.Add(roomId);
            return Task.CompletedTask;
        }
    }

    private sealed class SequencedSharedRoomAuthorizationRepository(
        params IReadOnlyList<RoomId>[] snapshots) : ISharedRoomAuthorizationRepository
    {
        private int _index;

        public Task<IReadOnlyList<RoomId>> GetSharedActiveRoomIdsAsync(
            UserId firstUserId,
            UserId secondUserId,
            CancellationToken ct = default)
        {
            var snapshot = snapshots[Math.Min(_index, snapshots.Length - 1)];
            _index++;
            return Task.FromResult(snapshot);
        }

        public Task<IReadOnlyList<RoomId>> GetHistoricalArtifactRoomIdsAsync(
            UserId readerUserId,
            UserId authorUserId,
            CancellationToken ct = default) => Task.FromResult<IReadOnlyList<RoomId>>([]);
    }

    private sealed class FakeSharedRoomAuthorizationRepository(
        params RoomId[] sharedRooms) : ISharedRoomAuthorizationRepository
    {
        public Task<IReadOnlyList<RoomId>> GetSharedActiveRoomIdsAsync(
            UserId firstUserId,
            UserId secondUserId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<RoomId>>(sharedRooms);

        public Task<IReadOnlyList<RoomId>> GetHistoricalArtifactRoomIdsAsync(
            UserId readerUserId,
            UserId authorUserId,
            CancellationToken ct = default) => Task.FromResult<IReadOnlyList<RoomId>>([]);
    }

    private sealed class HistoricalArtifactAuthorizationRepository(
        UserId readerId,
        UserId authorId,
        params IReadOnlyList<RoomId>[] snapshots) : ISharedRoomAuthorizationRepository
    {
        private int _index;

        public Task<IReadOnlyList<RoomId>> GetSharedActiveRoomIdsAsync(
            UserId firstUserId,
            UserId secondUserId,
            CancellationToken ct = default) => Task.FromResult<IReadOnlyList<RoomId>>([]);

        public Task<IReadOnlyList<RoomId>> GetHistoricalArtifactRoomIdsAsync(
            UserId readerUserId,
            UserId authorUserId,
            CancellationToken ct = default)
        {
            if (readerUserId != readerId || authorUserId != authorId)
            {
                return Task.FromResult<IReadOnlyList<RoomId>>([]);
            }
            return Task.FromResult(snapshots[Math.Min(_index++, snapshots.Length - 1)]);
        }
    }

    private sealed class RecordingIdentityExposureRepository : IIdentityExposureRepository
    {
        public List<(UserId Owner, UserId Recipient)> Records { get; } = [];

        public Task RecordAsync(
            UserId identityOwnerUserId,
            UserId recipientUserId,
            DateTimeOffset exposedAt,
            CancellationToken ct = default)
        {
            Records.Add((identityOwnerUserId, recipientUserId));
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<UserId>> GetHistoricalPeerUserIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<IReadOnlyList<IdentityLifecycleProjection>> GetLifecycleSnapshotForRecipientAsync(
            UserId recipientUserId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<IdentityLifecycleProjection>>([]);
    }

    private static User CreateLifecycleUser(UserId userId)
    {
        var user = User.Create(
            userId,
            $"{userId.Value:N}@example.test",
            $"identity-{userId.Value:N}",
            "Identity user");
        user.AdvanceIdentityLifecycle(Guid.CreateVersion7());
        return user;
    }

    private static UserDevice CreateCertifiedDevice(UserId userId, string deviceId)
    {
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: DummyKemKey,
            signingPublicKey: DummySignKey,
            deviceLabel: $"Test device {deviceId}",
            signerDeviceId: deviceId,
            issuedAt: DateTimeOffset.UtcNow,
            expiresAt: null);
        return device;
    }

    [Fact]
    public async Task Filters_Out_Active_Device_Not_In_Signed_List()
    {
        var userId = UserId.New();
        var listed = CreateCertifiedDevice(userId, "device-1");
        var staleRow = CreateCertifiedDevice(userId, "device-2");
        var list = CreateDeviceList(userId, generation: 2, signerDeviceId: "device-1");

        var service = CreateService(devices: [listed, staleRow], lists: [list]);

        var bundle = await service.GetAsync(userId, userId, TestContext.Current.CancellationToken);

        Assert.NotNull(bundle);
        Assert.Single(bundle.Devices);
        Assert.Equal("device-1", bundle.Devices[0].DeviceId);
    }

    private static UserDeviceList CreateDeviceList(
        UserId userId,
        long generation,
        string signerDeviceId)
    {
        return TestDeviceList.Create(
            userId,
            generation,
            deviceIdsJson: $"[{{\"deviceId\":\"{signerDeviceId}\",\"signerDeviceId\":\"{signerDeviceId}\"}}]",
            signerDeviceId,
            signature: DummySignature,
            issuedAtMs: DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            expiresAtMs: null);
    }
}
