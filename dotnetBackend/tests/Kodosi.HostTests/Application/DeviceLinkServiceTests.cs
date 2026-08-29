using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class DeviceLinkServiceTests
{
    [Fact]
    public async Task PollAsync_Replays_Approved_Result_Until_Acknowledged()
    {
        var row = ApprovedLinkRequest();
        var repository = new PollRepository(row);

        var service = CreatePollService(repository, row);

        var response = await service.PollAsync(
            row.UserId,
            row.DeviceCode,
            TestContext.Current.CancellationToken);

        Assert.Equal(DeviceLinkPollState.Approved, response.State);
        Assert.Equal(7, response.DeviceListGeneration);
        Assert.NotNull(response.IdentityBundle);

        var replay = await service.PollAsync(
            row.UserId,
            row.DeviceCode,
            TestContext.Current.CancellationToken);
        Assert.Equal(DeviceLinkPollState.Approved, replay.State);
    }

    [Fact]
    public async Task PollAsync_Rejects_Corrupt_Certificate_Signature()
    {
        var row = ApprovedLinkRequest();
        var device = CertifiedDevice(row.UserId, row.DeviceId, "signer-device");
        DomainFixtureHydrator.SetDeviceCertificateSignature(device, [1]);
        var list = TestDeviceList.Create(
            row.UserId,
            7,
            TestDeviceList.Entries([(row.DeviceId, "signer-device")]),
            row.DeviceId,
            [0x17],
            1_700_000_000_000,
            null);
        var service = new DeviceLinkPollService(
            new PollRepository(row),
            new HistoricalListRepository(list),
            new FakeUserDeviceRepository(device),
            new FakeUserRepository(EnrolledUser(row.UserId)),
            new FakeUserLifecycleLock(),
            new FakeUnitOfWork());

        await Assert.ThrowsAsync<DeviceCertificateCorruptionException>(() =>
            service.PollAsync(
                row.UserId,
                row.DeviceCode,
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task PollAsync_Does_Not_Expose_Result_To_Another_User()
    {
        var row = ApprovedLinkRequest();
        var repository = new PollRepository(row);

        var result = await CreatePollService(repository, row).PollAsync(
            UserId.New(),
            row.DeviceCode,
            TestContext.Current.CancellationToken);

        Assert.Equal(DeviceLinkPollState.NotFound, result.State);
    }

    [Fact]
    public async Task PollAsync_Returns_Expired_Without_Consuming_Approved_Row()
    {
        var row = PendingLinkRequest(ttl: TimeSpan.FromMilliseconds(1));
        await Task.Delay(10, TestContext.Current.CancellationToken);
        var repository = new PollRepository(row);

        var response = await CreatePollService(repository, row).PollAsync(
            row.UserId,
            row.DeviceCode,
            TestContext.Current.CancellationToken);

        Assert.Equal(DeviceLinkPollState.Expired, response.State);
        Assert.Null(response.DeviceListGeneration);
    }

    [Fact]
    public async Task PollAsync_Returns_Retained_Cancelled_State()
    {
        var row = PendingLinkRequest();
        DomainFixtureHydrator.CancelDeviceLink(row, DateTimeOffset.UtcNow);
        var repository = new PollRepository(row);

        var response = await CreatePollService(repository, row).PollAsync(
            row.UserId,
            row.DeviceCode,
            TestContext.Current.CancellationToken);

        Assert.Equal(DeviceLinkPollState.Cancelled, response.State);
    }

    [Fact]
    public async Task Approved_Request_Rejects_Cancellation_And_Remains_Pollable()
    {
        var row = ApprovedLinkRequest();
        var repository = new PollRepository(row);
        var requestService = CreateRequestService(repository);

        await Assert.ThrowsAsync<ConflictException>(() => requestService.CancelAsync(
            row.UserId,
            row.UserCode,
            TestContext.Current.CancellationToken));
        var response = await CreatePollService(repository, row).PollAsync(
            row.UserId,
            row.DeviceCode,
            TestContext.Current.CancellationToken);

        Assert.Equal(DeviceLinkPollState.Approved, response.State);
        Assert.Null(row.CancelledAt);
    }

    [Fact]
    public async Task LoadApprovedBundleAsync_Projects_The_Approved_Historical_Generation()
    {
        var userId = UserId.New();
        var approvedDevice = CertifiedDevice(userId, "approved-device", "approved-signer");
        var unrelatedDevice = CertifiedDevice(userId, "later-device", "later-signer");
        unrelatedDevice.Revoke("approved-signer");
        var approvedList = TestDeviceList.Create(
            userId,
            7,
            "[{\"deviceId\":\"approved-device\",\"signerDeviceId\":\"approved-signer\"}]",
            "approved-device",
            [0x17],
            1_700_000_000_000,
            null);
        var latestList = TestDeviceList.Create(
            userId,
            8,
            "[{\"deviceId\":\"later-device\",\"signerDeviceId\":\"later-signer\"}]",
            "later-device",
            [0x18],
            1_700_000_001_000,
            null);
        var lists = new HistoricalListRepository(approvedList, latestList);
        var devices = new FakeUserDeviceRepository(approvedDevice, unrelatedDevice);

        var users = new FakeUserRepository(EnrolledUser(userId));

        var row = DeviceLinkRequest.Create(
            userId,
            "device-secret",
            "ABCD-EFGH",
            "approved-device",
            "Laptop",
            new byte[1184],
            new byte[1952],
            TimeSpan.FromMinutes(15),
            DateTimeOffset.UtcNow);
        row.Approve(7, DateTimeOffset.UtcNow);
        var service = new DeviceLinkPollService(
            new PollRepository(row),
            lists,
            devices,
            users,
            new FakeUserLifecycleLock(),
            new FakeUnitOfWork());
        var result = await service.PollAsync(
            userId,
            row.DeviceCode,
            TestContext.Current.CancellationToken);
        var bundle = Assert.IsType<UserIdentityBundleResponse>(result.IdentityBundle);

        var returnedList = new SignedDeviceListParser().Parse(
            Convert.FromBase64String(bundle.DeviceList.Body));
        Assert.Equal(7, returnedList.Generation);
        Assert.Equal(
            Convert.ToBase64String(approvedDevice.DeviceCertificate!),
            Assert.Single(bundle.Devices).Certificate);
        Assert.Equal(
            Convert.ToBase64String(unrelatedDevice.DeviceCertificate!),
            Assert.Single(bundle.HistoricalDevices).Certificate);
        var json = JsonSerializer.SerializeToElement(
            bundle,
            new JsonSerializerOptions(JsonSerializerDefaults.Web));
        Assert.Equal(
            ["body", "signature"],
            json.GetProperty("deviceList").EnumerateObject().Select(property => property.Name));
        foreach (var certificate in json.GetProperty("devices").EnumerateArray()
                     .Concat(json.GetProperty("historicalDevices").EnumerateArray()))
        {
            Assert.Equal(
                ["certificate", "certificateSignature"],
                certificate.EnumerateObject().Select(property => property.Name));
        }
        Assert.Equal(7, lists.RequestedGeneration);
    }

    [Fact]
    public async Task AcknowledgeAsync_Rejects_Receipt_Invalidated_By_Identity_Reset()
    {
        var row = ApprovedLinkRequest();
        DomainFixtureHydrator.CancelDeviceLink(row, DateTimeOffset.UtcNow);
        var repository = new PollRepository(row);

        var service = CreateRequestService(repository);

        await Assert.ThrowsAsync<DeviceLinkReceiptInvalidatedException>(() =>
            service.AcknowledgeAsync(
                row.UserId,
                row.DeviceCode,
                row.DeviceId,
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task AcknowledgeAsync_Is_Idempotent_And_Bound_To_User_And_Device()
    {
        var row = ApprovedLinkRequest();
        var repository = new PollRepository(row);
        var service = CreateRequestService(repository);

        var first = await service.AcknowledgeAsync(
            row.UserId,
            row.DeviceCode,
            row.DeviceId,
            TestContext.Current.CancellationToken);
        var second = await service.AcknowledgeAsync(
            row.UserId,
            row.DeviceCode,
            row.DeviceId,
            TestContext.Current.CancellationToken);
        var unrelated = await service.AcknowledgeAsync(
            UserId.New(),
            row.DeviceCode,
            row.DeviceId,
            TestContext.Current.CancellationToken);

        Assert.Equal(DeviceLinkAcknowledgeResult.Acknowledged, first);
        Assert.Equal(DeviceLinkAcknowledgeResult.Acknowledged, second);
        Assert.Equal(DeviceLinkAcknowledgeResult.NotFound, unrelated);
        Assert.Equal(2, repository.AcknowledgeCalls);
    }

    private static DeviceLinkRequestService CreateRequestService(
        IDeviceLinkRequestRepository repository) =>
        new(
            repository,
            new FakeUserLifecycleLock(),
            new FakeUnitOfWork(),
            TimeProvider.System);

    private static DeviceLinkPollService CreatePollService(
        IDeviceLinkRequestRepository repository,
        DeviceLinkRequest row)
    {
        var device = CertifiedDevice(row.UserId, row.DeviceId, "signer-device");
        var list = TestDeviceList.Create(
            row.UserId,
            7,
            TestDeviceList.Entries([(row.DeviceId, "signer-device")]),
            row.DeviceId,
            [0x17],
            1_700_000_000_000,
            null);
        return new DeviceLinkPollService(
            repository,
            new HistoricalListRepository(list),
            new FakeUserDeviceRepository(device),
            new FakeUserRepository(EnrolledUser(row.UserId)),
            new FakeUserLifecycleLock(),
            new FakeUnitOfWork());
    }

    private static DeviceLinkRequest ApprovedLinkRequest(TimeSpan? ttl = null)
    {
        var request = PendingLinkRequest(ttl);
        request.Approve(7, DateTimeOffset.UtcNow);
        return request;
    }

    private static DeviceLinkRequest PendingLinkRequest(TimeSpan? ttl = null)
    {
        return DeviceLinkRequest.Create(
            UserId.New(),
            "device-secret",
            "ABCD-EFGH",
            "device-1",
            "Laptop",
            new byte[1184],
            new byte[1952],
            ttl ?? TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
    }

    private static User EnrolledUser(UserId userId)
    {
        var user = User.Create(
            userId,
            $"{userId.Value:N}@example.test",
            $"device-{userId.Value:N}",
            "Device-link user");
        user.AdvanceIdentityLifecycle(Guid.CreateVersion7());
        return user;
    }

    private static UserDevice CertifiedDevice(
        UserId userId,
        string deviceId,
        string signerDeviceId)
    {
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Laptop",
            signerDeviceId: signerDeviceId,
            issuedAt: DateTimeOffset.FromUnixTimeMilliseconds(1_700_000_000_000),
            expiresAt: null);
        return device;
    }

    private sealed class HistoricalListRepository(params UserDeviceList[] lists)
        : IUserDeviceListRepository
    {
        private readonly IReadOnlyList<UserDeviceList> _lists = lists;

        public long? RequestedGeneration { get; private set; }

        public Task<UserDeviceList?> GetLatestAsync(
            UserId userId,
            CancellationToken ct = default) =>
            Task.FromResult(_lists
                .Where(list => list.UserId == userId)
                .OrderByDescending(list => list.Generation)
                .FirstOrDefault());

        public Task<UserDeviceList?> GetGenerationAsync(
            UserId userId,
            long generation,
            CancellationToken ct = default)
        {
            RequestedGeneration = generation;
            return Task.FromResult(_lists.SingleOrDefault(list =>
                list.UserId == userId && list.Generation == generation));
        }

        public Task<IReadOnlyList<UserDeviceList>> GetLatestByUserIdsAsync(
            IReadOnlyCollection<UserId> userIds,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task AddAsync(UserDeviceList list, CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<int> RemoveAllForUserAsync(
            UserId userId,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }

    private static bool TryCancelFixture(
        DeviceLinkRequest request,
        DateTimeOffset cancelledAt)
    {
        if (request.AcknowledgedAt.HasValue || request.CancelledAt.HasValue)
        {
            return false;
        }
        DomainFixtureHydrator.CancelDeviceLink(request, cancelledAt);
        return true;
    }

    private sealed class PollRepository(DeviceLinkRequest? row)
        : IDeviceLinkRequestRepository
    {
        public int AcknowledgeCalls { get; private set; }

        public Task AddAsync(DeviceLinkRequest request, CancellationToken ct = default)
            => throw new NotSupportedException();

        public Task<DeviceLinkRequest?> GetByDeviceCodeAsync(
            string deviceCode,
            CancellationToken ct = default)
            => Task.FromResult(row is not null && row.DeviceCode == deviceCode ? row : null);

        public Task<DeviceLinkRequest?> GetByUserCodeAsync(
            string userCode,
            CancellationToken ct = default)
            => throw new NotSupportedException();

        public Task<bool> IsUserCodeAvailableAsync(string userCode, CancellationToken ct = default)
            => throw new NotSupportedException();

        public Task<IReadOnlyList<DeviceLinkRequest>> ListPendingForUserAsync(
            UserId userId,
            DateTimeOffset now,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<DeviceLinkRequest>>(
                row is not null && row.UserId == userId && row.IsPending(now) ? [row] : []);

        public void Update(DeviceLinkRequest request)
            => throw new NotSupportedException();

        public Task<DeviceLinkCancelOutcome> CancelPendingAsync(
            string userCode,
            UserId userId,
            DateTimeOffset cancelledAt,
            CancellationToken ct = default)
        {
            if (row is null || row.UserCode != userCode || row.UserId != userId)
            {
                return Task.FromResult(DeviceLinkCancelOutcome.NotFound);
            }
            if (row.CancelledAt.HasValue)
            {
                return Task.FromResult(DeviceLinkCancelOutcome.AlreadyCancelled);
            }
            if (row.ApprovedAt.HasValue)
            {
                return Task.FromResult(DeviceLinkCancelOutcome.Approved);
            }
            if (!row.IsPending(cancelledAt))
            {
                return Task.FromResult(DeviceLinkCancelOutcome.Expired);
            }
            DomainFixtureHydrator.CancelDeviceLink(row, DateTimeOffset.UtcNow);
            return Task.FromResult(DeviceLinkCancelOutcome.Cancelled);
        }

        public Task<DeviceLinkAcknowledgeOutcome> AcknowledgeApprovedAsync(
            Guid requestId,
            UserId userId,
            string deviceId,
            DateTimeOffset now,
            CancellationToken ct = default)
        {
            AcknowledgeCalls++;
            if (row?.CancelledAt is not null)
            {
                return Task.FromResult(DeviceLinkAcknowledgeOutcome.Invalidated);
            }
            return Task.FromResult(DeviceLinkAcknowledgeOutcome.Acknowledged);
        }

        public Task<int> InvalidateApprovedBeforeGenerationAsync(
            UserId userId,
            long committedGeneration,
            Guid? excludingRequestId,
            DateTimeOffset invalidatedAt,
            CancellationToken ct = default)
        {
            var invalidated = row is not null
                && row.UserId == userId
                && row.Id != excludingRequestId
                && row.ApprovedAt.HasValue
                && !row.AcknowledgedAt.HasValue
                && !row.CancelledAt.HasValue
                && row.DeviceListGeneration < committedGeneration
                && TryCancelFixture(row, invalidatedAt);
            return Task.FromResult(invalidated ? 1 : 0);
        }

        public Task<int> InvalidateOutstandingForUserAsync(
            UserId userId,
            DateTimeOffset invalidatedAt,
            CancellationToken ct = default)
        {
            if (row is null || row.UserId != userId || !TryCancelFixture(row, invalidatedAt))
            {
                return Task.FromResult(0);
            }
            return Task.FromResult(1);
        }

        public Task<int> DeleteStaleAsync(DateTimeOffset now, CancellationToken ct = default)
            => throw new NotSupportedException();
    }
}
