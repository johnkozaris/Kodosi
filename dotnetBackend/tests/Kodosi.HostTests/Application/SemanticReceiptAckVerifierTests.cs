using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class SemanticReceiptAckVerifierTests
{
    [Fact]
    public async Task VerifyAsync_Uses_Exact_Canonical_Preimage_For_Active_Device()
    {
        var userId = UserId.New();
        var device = CreateDevice(userId, "device-1");
        var deviceList = CreateDeviceList(userId, "device-1");
        var signatureVerifier = new RecordingSignatureVerifier(result: true);
        var verifier = new SemanticReceiptAckVerifier(
            new FakeUserDeviceRepository(device),
            new FakeUserDeviceListRepository(deviceList),
            signatureVerifier,
            TimeProvider.System);
        var sessionId = SessionId.New();
        var incarnationId = Guid.CreateVersion7();
        var requestId = Guid.CreateVersion7();

        var valid = await verifier.VerifyAsync(
            sessionId,
            incarnationId,
            requestId,
            userId,
            "device-1",
            userId.Value.ToString(),
            "device-1",
            Convert.ToBase64String([1, 2, 3]),
            TestContext.Current.CancellationToken);

        Assert.True(valid);
        Assert.Equal(device.SigningPublicKey, signatureVerifier.PublicKey);
        Assert.Equal(
            SemanticReceiptAckPreimage.Create(
                sessionId,
                incarnationId,
                requestId,
                userId,
                "device-1"),
            signatureVerifier.Message);
        Assert.Equal([1, 2, 3], signatureVerifier.Signature);
    }

    [Fact]
    public async Task VerifyAsync_Rejects_Mismatched_Authenticated_Target_Before_Signature_Check()
    {
        var userId = UserId.New();
        var device = CreateDevice(userId, "device-1");
        var signatureVerifier = new RecordingSignatureVerifier(result: true);
        var verifier = new SemanticReceiptAckVerifier(
            new FakeUserDeviceRepository(device),
            new FakeUserDeviceListRepository(CreateDeviceList(userId, "device-1")),
            signatureVerifier,
            TimeProvider.System);

        var valid = await verifier.VerifyAsync(
            SessionId.New(),
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            userId,
            "device-1",
            UserId.New().Value.ToString(),
            "device-1",
            Convert.ToBase64String([1]),
            TestContext.Current.CancellationToken);

        Assert.False(valid);
        Assert.Null(signatureVerifier.Message);
    }

    [Theory]
    [InlineData(true, false)]
    [InlineData(false, true)]
    public async Task VerifyAsync_Rejects_Empty_Identity_Before_Repository_IO(
        bool emptySession,
        bool emptyUser)
    {
        var userId = emptyUser ? UserId.From(Guid.Empty) : UserId.New();
        var devices = new CountingUserDeviceRepository();
        var lists = new CountingUserDeviceListRepository();
        var signatureVerifier = new RecordingSignatureVerifier(result: true);
        var verifier = new SemanticReceiptAckVerifier(
            devices,
            lists,
            signatureVerifier,
            TimeProvider.System);

        var valid = await verifier.VerifyAsync(
            emptySession ? SessionId.From(Guid.Empty) : SessionId.New(),
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            userId,
            "device-1",
            userId.Value.ToString(),
            "device-1",
            Convert.ToBase64String([1]),
            TestContext.Current.CancellationToken);

        Assert.False(valid);
        Assert.Equal(0, devices.ReadCount);
        Assert.Equal(0, lists.ReadCount);
        Assert.Null(signatureVerifier.Message);
    }

    [Theory]
    [InlineData("")]
    [InlineData("not-base64")]
    public async Task VerifyAsync_Rejects_Invalid_Signature_Encoding(string signature)
    {
        var userId = UserId.New();
        var signatureVerifier = new RecordingSignatureVerifier(result: true);
        var verifier = new SemanticReceiptAckVerifier(
            new FakeUserDeviceRepository(CreateDevice(userId, "device-1")),
            new FakeUserDeviceListRepository(CreateDeviceList(userId, "device-1")),
            signatureVerifier,
            TimeProvider.System);

        Assert.False(await verifier.VerifyAsync(
            SessionId.New(),
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            userId,
            "device-1",
            userId.Value.ToString(),
            "device-1",
            signature,
            TestContext.Current.CancellationToken));
        Assert.Null(signatureVerifier.Message);
    }

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

    private sealed class CountingUserDeviceRepository : IUserDeviceRepository
    {
        public int ReadCount { get; private set; }

        public Task<IReadOnlyList<UserDevice>> GetByUserIdAsync(
            UserId userId,
            CancellationToken ct = default)
        {
            ReadCount++;
            return Task.FromResult<IReadOnlyList<UserDevice>>([]);
        }

        public Task<UserDevice?> GetByDeviceIdAsync(
            string deviceId,
            CancellationToken ct = default)
        {
            ReadCount++;
            return Task.FromResult<UserDevice?>(null);
        }

        public Task<IReadOnlyDictionary<string, UserId>> GetUserIdsByDeviceIdsAsync(
            IReadOnlyCollection<string> deviceIds,
            CancellationToken ct = default)
        {
            ReadCount++;
            return Task.FromResult<IReadOnlyDictionary<string, UserId>>(
                new Dictionary<string, UserId>());
        }

        public Task<IReadOnlyList<UserDevice>> GetByUserIdsAsync(
            IReadOnlyList<UserId> userIds,
            CancellationToken ct = default)
        {
            ReadCount++;
            return Task.FromResult<IReadOnlyList<UserDevice>>([]);
        }

        public Task AddAsync(UserDevice device, CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task<int> RemoveAllForUserAsync(UserId userId, CancellationToken ct = default) =>
            Task.FromResult(0);

        public void Update(UserDevice device) { }
    }

    private sealed class CountingUserDeviceListRepository : IUserDeviceListRepository
    {
        public int ReadCount { get; private set; }

        public Task<UserDeviceList?> GetLatestAsync(
            UserId userId,
            CancellationToken ct = default)
        {
            ReadCount++;
            return Task.FromResult<UserDeviceList?>(null);
        }

        public Task<UserDeviceList?> GetGenerationAsync(
            UserId userId,
            long generation,
            CancellationToken ct = default)
        {
            ReadCount++;
            return Task.FromResult<UserDeviceList?>(null);
        }

        public Task<IReadOnlyList<UserDeviceList>> GetLatestByUserIdsAsync(
            IReadOnlyCollection<UserId> userIds,
            CancellationToken ct = default)
        {
            ReadCount++;
            return Task.FromResult<IReadOnlyList<UserDeviceList>>([]);
        }

        public Task AddAsync(UserDeviceList list, CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task<int> RemoveAllForUserAsync(UserId userId, CancellationToken ct = default) =>
            Task.FromResult(0);
    }

    private sealed class RecordingSignatureVerifier(bool result) : IPopSignatureVerifier
    {
        public byte[]? PublicKey { get; private set; }
        public byte[]? Message { get; private set; }
        public byte[]? Signature { get; private set; }

        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature)
        {
            PublicKey = publicKey.ToArray();
            Message = message.ToArray();
            Signature = signature.ToArray();
            return result;
        }
    }
}
