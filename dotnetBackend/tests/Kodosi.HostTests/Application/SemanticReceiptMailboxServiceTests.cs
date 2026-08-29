using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class SemanticReceiptMailboxServiceTests
{
    [Fact]
    public async Task Upload_Requires_Exact_Active_Owner_Device_Without_Session_Lookup()
    {
        var user = UserId.New();
        var repository = new RecordingRepository { StoreResult = true };
        var service = CreateService(user, "device-1", repository);
        var receipt = Envelope(user, "device-1");

        var outcome = await service.UploadAsync(
            user,
            "device-1",
            receipt,
            TestContext.Current.CancellationToken);

        Assert.Equal(SemanticReceiptUploadOutcome.Stored, outcome);
        Assert.Equal(receipt.SessionId, repository.Stored?.SessionId);
        Assert.Equal(receipt.IncarnationId, repository.Stored?.IncarnationId);
        Assert.Equal(receipt.RequestId, repository.Stored?.RequestId);
    }

    [Fact]
    public async Task Upload_Rejects_Cross_Account_Or_Device_Target()
    {
        var user = UserId.New();
        var repository = new RecordingRepository { StoreResult = true };
        var service = CreateService(user, "device-1", repository);

        Assert.Equal(
            SemanticReceiptUploadOutcome.UnauthorizedDevice,
            await service.UploadAsync(
                user,
                "device-1",
                Envelope(UserId.New(), "device-1"),
                TestContext.Current.CancellationToken));
        Assert.Equal(
            SemanticReceiptUploadOutcome.UnauthorizedDevice,
            await service.UploadAsync(
                user,
                "device-1",
                Envelope(user, "other-device"),
                TestContext.Current.CancellationToken));
        Assert.Null(repository.Stored);
    }

    [Fact]
    public async Task List_Is_Exact_Device_Scoped_And_Bounded()
    {
        var user = UserId.New();
        var repository = new RecordingRepository();
        var service = CreateService(user, "device-1", repository);

        var page = await service.ListAsync(
            user,
            "device-1",
            17,
            null,
            TestContext.Current.CancellationToken);

        Assert.NotNull(page);
        Assert.Empty(page.Items);
        Assert.Null(page.NextCursor);
        Assert.Equal((user, "device-1", 18, null), repository.ListTarget);
        Assert.Null(await service.ListAsync(
            user,
            "unknown-device",
            17,
            null,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task List_Emits_A_Stable_Continuation_Past_A_Full_Page()
    {
        var user = UserId.New();
        var repository = new RecordingRepository();
        var first = Receipt(user, "device-1", DateTimeOffset.UnixEpoch);
        var second = Receipt(user, "device-1", DateTimeOffset.UnixEpoch.AddSeconds(1));
        repository.ListResults = [first, second];
        var service = CreateService(user, "device-1", repository);

        var firstPage = await service.ListAsync(
            user,
            "device-1",
            1,
            null,
            TestContext.Current.CancellationToken);

        Assert.NotNull(firstPage);
        Assert.Equal([first], firstPage.Items);
        Assert.NotNull(firstPage.NextCursor);
        var cursor = SemanticReceiptCursor.Decode(firstPage.NextCursor);
        Assert.Equal(first.StoredAt, cursor?.StoredAt);
        Assert.Equal(first.Id, cursor?.Id);
        Assert.Equal((user, "device-1", 2, null), repository.ListTarget);

        repository.ListResults = [second];
        var secondPage = await service.ListAsync(
            user,
            "device-1",
            1,
            firstPage.NextCursor,
            TestContext.Current.CancellationToken);

        Assert.NotNull(secondPage);
        Assert.Equal([second], secondPage.Items);
        Assert.Null(secondPage.NextCursor);
        Assert.Equal((user, "device-1", 2, cursor), repository.ListTarget);
    }

    [Fact]
    public async Task Ack_Verifies_Exact_Device_Signature_Before_Mutation()
    {
        var user = UserId.New();
        var repository = new RecordingRepository { AckResult = true };
        var service = CreateService(user, "device-1", repository);
        var receipt = Envelope(user, "device-1");
        var ack = new SemanticReceiptAcknowledgement(
            receipt.SessionId,
            receipt.IncarnationId,
            receipt.RequestId,
            user.Value.ToString(),
            "device-1",
            Convert.ToBase64String([1]));

        var outcome = await service.AcknowledgeAsync(
            user,
            "device-1",
            ack,
            TestContext.Current.CancellationToken);

        Assert.Equal(SemanticReceiptAckOutcome.Acknowledged, outcome);
        Assert.Equal(
            (receipt.SessionId, receipt.IncarnationId, user, "device-1", receipt.RequestId),
            repository.Acknowledged);
    }

    private static SemanticReceiptMailboxService CreateService(
        UserId user,
        string deviceId,
        ISemanticRelayRepository repository)
    {
        var device = CreateDevice(user, deviceId);
        var devices = new FakeUserDeviceRepository(device);
        var lists = new FakeUserDeviceListRepository(CreateDeviceList(user, deviceId));
        var signatureVerifier = new AlwaysValidSignatureVerifier();
        return new SemanticReceiptMailboxService(
            repository,
            new SemanticReceiptVerifier(
                devices,
                lists,
                signatureVerifier,
                TimeProvider.System),
            new SemanticReceiptAckVerifier(
                devices,
                lists,
                signatureVerifier,
                TimeProvider.System),
            devices,
            lists,
            TimeProvider.System);
    }

    private static SemanticReceiptEnvelope Envelope(UserId user, string deviceId) =>
        new(
            SessionId.New(),
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "steer",
            new string('a', 64),
            "injected",
            user,
            deviceId,
            user,
            deviceId,
            Convert.ToBase64String([1]));

    private static SemanticRelayReceipt Receipt(
        UserId user,
        string deviceId,
        DateTimeOffset storedAt) =>
        SemanticRelayReceipt.Create(
            Guid.CreateVersion7(),
            SessionId.New(),
            Guid.CreateVersion7(),
            user,
            deviceId,
            Guid.CreateVersion7(),
            "steer",
            new string('a', 64),
            "injected",
            user,
            deviceId,
            Convert.ToBase64String([1]),
            storedAt);

    private static UserDevice CreateDevice(UserId userId, string deviceId)
    {
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: Enumerable.Repeat((byte)7, 1952).ToArray(),
            deviceLabel: "Device",
            signerDeviceId: deviceId,
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: null);
        return device;
    }

    private static UserDeviceList CreateDeviceList(UserId userId, string deviceId) =>
        TestDeviceList.Create(
            userId,
            1,
            $$"""[{"deviceId":"{{deviceId}}","signerDeviceId":"{{deviceId}}"}]""",
            deviceId,
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            null);

    private sealed class RecordingRepository : ISemanticRelayRepository
    {
        public bool StoreResult { get; init; }
        public bool AckResult { get; init; }
        public IReadOnlyList<SemanticRelayReceipt> ListResults { get; set; } = [];
        public SemanticReceiptEnvelope? Stored { get; private set; }
        public (UserId, string, int, SemanticReceiptCursor?)? ListTarget { get; private set; }
        public (SessionId, Guid, UserId, string, Guid)? Acknowledged { get; private set; }

        public Task<bool> StoreReceiptAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            string outcome,
            UserId ownerUserId,
            string ownerDeviceId,
            string signature,
            CancellationToken ct = default)
        {
            Stored = new SemanticReceiptEnvelope(
                sessionId,
                incarnationId,
                requestId,
                mode,
                payloadSha256,
                outcome,
                requesterUserId,
                requesterDeviceId,
                ownerUserId,
                ownerDeviceId,
                signature);
            return Task.FromResult(StoreResult);
        }

        public Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsForDeviceAsync(
            UserId requesterUserId,
            string requesterDeviceId,
            int limit,
            SemanticReceiptCursor? cursor = null,
            CancellationToken ct = default)
        {
            ListTarget = (requesterUserId, requesterDeviceId, limit, cursor);
            return Task.FromResult(ListResults);
        }

        public Task<bool> AcknowledgeReceiptAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            CancellationToken ct = default)
        {
            Acknowledged = (
                sessionId,
                incarnationId,
                requesterUserId,
                requesterDeviceId,
                requestId);
            return Task.FromResult(AckResult);
        }

        public Task<SemanticRequestClaim> ClaimRequestAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<SemanticRequestClaim?> FindExactRequestAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            Guid requestId,
            string mode,
            string payloadSha256,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task MarkDispatchedAsync(Guid requestRowId, CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<SemanticRelayReceipt>> ListPendingReceiptsAsync(
            SessionId sessionId,
            Guid incarnationId,
            UserId requesterUserId,
            string requesterDeviceId,
            int limit,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<SemanticRelayReceipt>>([]);

        public Task<int> DeleteAcknowledgedBeforeAsync(
            DateTimeOffset cutoff,
            int limit,
            CancellationToken ct = default) =>
            throw new NotSupportedException();
    }
}
