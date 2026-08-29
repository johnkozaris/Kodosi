using Kodosi.Application;
using Kodosi.Domain;

using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class SessionKeyQueryServiceTests
{
    [Fact]
    public async Task GetMySessionKeyAsync_Returns_Ready_When_Blob_Exists()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var device = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var blob = CreateBlob(session.Id, device.DeviceId);
        var service = CreateSessionKeyQueryService(
            session,
            userDeviceRepository: new FakeUserDeviceRepository(device),
            userDeviceListRepository: new FakeUserDeviceListRepository(CreateDeviceList(device)),
            sessionKeyBlobRepository: new FakeSessionKeyBlobRepository(blob));

        var result = await service.GetMySessionKeyAsync(
            session.Id,
            ownerId,
            device.DeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionKeyFetchState.Ready, result.State);
        Assert.NotNull(result.Blob);
        Assert.Same(blob, result.Blob);
        Assert.Equal(blob.RecipientDeviceId, result.Blob!.RecipientDeviceId);
        Assert.Equal(blob.EncryptedSessionKey, result.Blob.EncryptedSessionKey);
    }

    [Fact]
    public async Task GetMySessionKeyAsync_Returns_PendingDistribution_When_Blob_Is_From_Previous_Generation()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var device = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var staleBlob = CreateBlob(session.Id, device.DeviceId);
        session.FenceKeyPublication();
        var service = CreateSessionKeyQueryService(
            session,
            userDeviceRepository: new FakeUserDeviceRepository(device),
            userDeviceListRepository: new FakeUserDeviceListRepository(CreateDeviceList(device)),
            sessionKeyBlobRepository: new FakeSessionKeyBlobRepository(staleBlob));

        var result = await service.GetMySessionKeyAsync(
            session.Id,
            ownerId,
            device.DeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionKeyFetchState.PendingDistribution, result.State);
        Assert.Null(result.Blob);
        Assert.Equal(new byte[] { 1, 2, 3 }, staleBlob.EncryptedSessionKey);
    }

    [Fact]
    public async Task GetMySessionKeyAsync_Returns_PendingDistribution_When_Live_And_Blob_Missing()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var device = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var service = CreateSessionKeyQueryService(
            session,
            userDeviceRepository: new FakeUserDeviceRepository(device),
            userDeviceListRepository: new FakeUserDeviceListRepository(CreateDeviceList(device)));

        var result = await service.GetMySessionKeyAsync(
            session.Id,
            ownerId,
            device.DeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionKeyFetchState.PendingDistribution, result.State);
        Assert.Null(result.Blob);
    }

    [Fact]
    public async Task ShouldRequestHostKeyDistributionAsync_Rechecks_Current_Live_Status()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var service = CreateSessionKeyQueryService(session);

        Assert.True(await service.ShouldRequestHostKeyDistributionAsync(
            session.Id,
            TestContext.Current.CancellationToken));

        session.End();

        Assert.False(await service.ShouldRequestHostKeyDistributionAsync(
            session.Id,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task GetMySessionKeyAsync_Returns_SessionNotLive_When_Blob_Missing_And_Session_Ended()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        session.End();
        var device = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var service = CreateSessionKeyQueryService(
            session,
            userDeviceRepository: new FakeUserDeviceRepository(device));

        var result = await service.GetMySessionKeyAsync(
            session.Id,
            ownerId,
            device.DeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionKeyFetchState.SessionNotLive, result.State);
        Assert.Null(result.Blob);
    }

    [Fact]
    public async Task GetMySessionKeyAsync_Returns_SessionNotLive_When_Ended_Even_If_Blob_Still_Exists()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        session.End();
        var device = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var orphanBlob = CreateBlob(session.Id, device.DeviceId);
        var service = CreateSessionKeyQueryService(
            session,
            userDeviceRepository: new FakeUserDeviceRepository(device),
            userDeviceListRepository: new FakeUserDeviceListRepository(CreateDeviceList(device)),
            sessionKeyBlobRepository: new FakeSessionKeyBlobRepository(orphanBlob));

        var result = await service.GetMySessionKeyAsync(
            session.Id,
            ownerId,
            device.DeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionKeyFetchState.SessionNotLive, result.State);
        Assert.Null(result.Blob);
    }

    [Fact]
    public async Task GetMySessionKeyAsync_Returns_UnknownDevice_When_Device_Is_Not_Registered()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var service = CreateSessionKeyQueryService(session);

        var result = await service.GetMySessionKeyAsync(
            session.Id,
            ownerId,
            $"{ownerId.Value}:desktop",
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionKeyFetchState.UnknownDevice, result.State);
        Assert.Null(result.Blob);
    }

    [Fact]
    public async Task GetMySessionKeyAsync_Uses_Injected_Time_For_Device_Expiry()
    {
        var now = DateTimeOffset.UtcNow;
        var timeProvider = new TestTimeProvider(now);
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var device = CreateDevice(
            ownerId,
            $"{ownerId.Value}:desktop",
            expiresAt: now.AddHours(1),
            issuedAt: now.AddMinutes(-1));
        var service = CreateSessionKeyQueryService(
            session,
            userDeviceRepository: new FakeUserDeviceRepository(device),
            userDeviceListRepository: new FakeUserDeviceListRepository(CreateDeviceList(device)),
            sessionKeyBlobRepository: new FakeSessionKeyBlobRepository(
                CreateBlob(session.Id, device.DeviceId)),
            timeProvider: timeProvider);

        var beforeExpiry = await service.GetMySessionKeyAsync(
            session.Id,
            ownerId,
            device.DeviceId,
            TestContext.Current.CancellationToken);
        timeProvider.Advance(TimeSpan.FromHours(2));
        var afterExpiry = await service.GetMySessionKeyAsync(
            session.Id,
            ownerId,
            device.DeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(SessionKeyFetchState.Ready, beforeExpiry.State);
        Assert.Equal(SessionKeyFetchState.UnknownDevice, afterExpiry.State);
    }

    [Fact]
    public async Task GetMySessionKeyAsync_Throws_NotFound_For_Unauthorized_User()
    {
        var ownerId = UserId.New();
        var outsiderId = UserId.New();
        var session = CreateSession(ownerId, SessionScope.JustMe);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        var device = CreateDevice(outsiderId, $"{outsiderId.Value}:desktop");
        var service = CreateSessionKeyQueryService(
            session,
            userDeviceRepository: new FakeUserDeviceRepository(device));

        await Assert.ThrowsAsync<NotFoundException>(() =>
            service.GetMySessionKeyAsync(
                session.Id,
                outsiderId,
                device.DeviceId,
                TestContext.Current.CancellationToken));
    }

    private static UserDevice CreateDevice(
        UserId userId,
        string deviceId,
        DateTimeOffset? expiresAt = null,
        DateTimeOffset? issuedAt = null)
    {
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Test device",
            signerDeviceId: deviceId,
            issuedAt: issuedAt ?? DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: expiresAt);
        return device;
    }

    private static UserDeviceList CreateDeviceList(UserDevice device) =>
        TestDeviceList.Create(
            device.UserId,
            1,
            $$"""[{"deviceId":"{{device.DeviceId}}","signerDeviceId":"{{device.DeviceId}}"}]""",
            device.DeviceId,
            [2],
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            null);

    private static SessionKeyBlob CreateBlob(SessionId sessionId, string deviceId) =>
        SessionKeyBlob.Create(
            sessionId,
            deviceId,
            [1, 2, 3],
            "sender-device",
            [4, 5, 6],
            0,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [10, 11, 12],
            issuedAtMs: 1_000L);
}
