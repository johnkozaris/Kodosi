using System.ComponentModel.DataAnnotations;
using System.Security.Cryptography;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Kodosi.Infrastructure.Crypto;

namespace Kodosi.HostTests;

public sealed class ArtifactEndorsementTests
{
    [Fact]
    public void PreimageUsesLengthPrefixesAndRfc4122IncarnationBytes()
    {
        var userId = UserId.From(Guid.Parse("00112233-4455-6677-8899-aabbccddeeff"));
        var incarnation = Guid.Parse("ffeeddcc-bbaa-9988-7766-554433221100");
        var preimage = ArtifactEndorsement.CreatePreimage(userId, incarnation, new string('a', 64), "device-1");
        var expected = Convert.FromHexString(
            "6b6f646f73693a61727469666163742d656e646f7273656d656e743a763100" +
            "00000024" + Convert.ToHexString(System.Text.Encoding.UTF8.GetBytes(userId.ToString())) +
            "ffeeddccbbaa99887766554433221100" +
            "00000040" + Convert.ToHexString(System.Text.Encoding.UTF8.GetBytes(new string('a', 64))) +
            "00000008" + "6465766963652d31");
        Assert.Equal(expected, preimage);
    }

    [Theory]
    [InlineData("")]
    [InlineData("ABCDEF")]
    [InlineData("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaA")]
    public void DigestMustBeCanonical(string digest)
    {
        Assert.Throws<DomainException>(() => ArtifactEndorsement.RequireDigest(digest));
        var request = new PutArtifactEndorsementRequest(Guid.NewGuid(), digest, "device-1", Convert.ToBase64String(new byte[3309]));
        var failures = new List<ValidationResult>();
        Assert.False(Validator.TryValidateObject(request, new ValidationContext(request), failures, true));
    }

    [Fact]
    public async Task PutIsIdempotentAndAllowsCurrentDeviceReendorsementAtCapacity()
    {
        var fixture = new Fixture();
        var digest = new string('a', 64);
        await fixture.PutAsync(digest);
        await fixture.PutAsync(digest);
        Assert.Single(fixture.Rows.Rows);
        fixture.Rows.CountOverride = ArtifactEndorsement.MaximumPerIdentity;
        await fixture.PutAsync(digest);
        await Assert.ThrowsAsync<ConflictException>(() => fixture.PutAsync(new string('b', 64)));
        Assert.Single(fixture.Rows.Rows);
        Assert.True(fixture.Lifecycle.Calls >= 4);
    }

    [Fact]
    public async Task PutRejectsWrongIdentityInactiveDeviceAndInvalidSignature()
    {
        var fixture = new Fixture();
        await Assert.ThrowsAsync<ConflictException>(() => fixture.Service.PutAsync(
            fixture.User.Id, Guid.NewGuid(), new string('a', 64), fixture.Device.DeviceId, new byte[3309], TestContext.Current.CancellationToken));
        fixture.Signatures.Accept = false;
        await Assert.ThrowsAsync<PolicyViolationException>(() => fixture.PutAsync(new string('a', 64)));
        fixture.Signatures.Accept = true;
        fixture.Device.Revoke("surviving-device");
        await Assert.ThrowsAsync<PolicyViolationException>(() => fixture.PutAsync(new string('a', 64)));
        Assert.Empty(fixture.Rows.Rows);
    }

    [Fact]
    public async Task PeerReadUsesIdentityPolicyAndResetHidesOldProofs()
    {
        var fixture = new Fixture();
        var digest = new string('a', 64);
        await fixture.PutAsync(digest);
        Assert.Single((await fixture.Service.GetAsync(fixture.User.Id, fixture.User.Id, digest, TestContext.Current.CancellationToken))!);
        Assert.Null(await fixture.Service.GetAsync(UserId.New(), fixture.User.Id, digest, TestContext.Current.CancellationToken));
        fixture.User.AdvanceIdentityLifecycle(Guid.NewGuid());
        Assert.Empty((await fixture.Service.ListAsync(fixture.User.Id, 0, 100, TestContext.Current.CancellationToken)).Items);
        await fixture.PutAsync(new string('b', 64));
        Assert.Single(fixture.Rows.Rows);
        Assert.Equal(fixture.User.IdentityIncarnationId, fixture.Rows.Rows[0].IdentityIncarnationId);
    }

    [Fact]
    public async Task AuthorizedPeerCanReadOnlyRequestedArtifact()
    {
        var fixture = new Fixture(areFriends: true);
        var digest = new string('a', 64);
        await fixture.PutAsync(digest);
        var caller = UserId.New();
        Assert.Single((await fixture.Service.GetAsync(caller, fixture.User.Id, digest, TestContext.Current.CancellationToken))!);
        Assert.Empty((await fixture.Service.GetAsync(caller, fixture.User.Id, new string('b', 64), TestContext.Current.CancellationToken))!);
    }

    [Fact]
    public async Task PaginationIsBoundedAndStable()
    {
        var fixture = new Fixture();
        await fixture.PutAsync(new string('b', 64));
        await fixture.PutAsync(new string('a', 64));
        var page = await fixture.Service.ListAsync(fixture.User.Id, 0, 1, TestContext.Current.CancellationToken);
        Assert.Equal(new string('a', 64), Assert.Single(page.Items).ArtifactDigest);
        Assert.True(page.HasMore);
        Assert.Equal(1, page.NextOffset);
        await Assert.ThrowsAsync<DomainException>(() => fixture.Service.ListAsync(fixture.User.Id, 0, 101, TestContext.Current.CancellationToken));
    }

    [Fact]
    public void NativeSignatureBindsUserIdentityDigestAndDevice()
    {
        if (!MLDsa.IsSupported) return;
        using var key = MLDsa.GenerateKey(MLDsaAlgorithm.MLDsa65);
        var userId = UserId.New();
        var incarnation = Guid.NewGuid();
        var digest = new string('a', 64);
        var preimage = ArtifactEndorsement.CreatePreimage(userId, incarnation, digest, "device-1");
        var signature = new byte[MLDsaAlgorithm.MLDsa65.SignatureSizeInBytes];
        key.SignData(preimage, signature, context: ReadOnlySpan<byte>.Empty);
        var verifier = new MLDsaPopSignatureVerifier();
        var publicKey = key.ExportMLDsaPublicKey();
        Assert.True(verifier.Verify(publicKey, preimage, signature));
        Assert.False(verifier.Verify(publicKey, ArtifactEndorsement.CreatePreimage(UserId.New(), incarnation, digest, "device-1"), signature));
        Assert.False(verifier.Verify(publicKey, ArtifactEndorsement.CreatePreimage(userId, Guid.NewGuid(), digest, "device-1"), signature));
        Assert.False(verifier.Verify(publicKey, ArtifactEndorsement.CreatePreimage(userId, incarnation, new string('b', 64), "device-1"), signature));
        Assert.False(verifier.Verify(publicKey, ArtifactEndorsement.CreatePreimage(userId, incarnation, digest, "device-2"), signature));
    }

    private sealed class Fixture
    {
        public User User { get; }
        public UserDevice Device { get; }
        public MemoryEndorsements Rows { get; } = new();
        public RecordingLifecycle Lifecycle { get; } = new();
        public RecordingSignatures Signatures { get; } = new();
        public ArtifactEndorsementService Service { get; }

        public Fixture(bool areFriends = false)
        {
            User = User.Create(UserId.New(), "endorsement@example.test", "endorsement", "Endorsement");
            User.AdvanceIdentityLifecycle(Guid.NewGuid());
            Device = TestDeviceCertificate.CreateDevice(User.Id, "device-1", new byte[1184], new byte[1952], "Device", "device-1", DateTimeOffset.UtcNow.AddMinutes(-1), null);
            var devices = new FakeUserDeviceRepository(Device);
            var users = new FakeUserRepository(User);
            var lists = new FakeUserDeviceListRepository(TestDeviceList.Create(User.Id, 1,
                "[{\"deviceId\":\"device-1\",\"signerDeviceId\":\"device-1\"}]", "device-1", new byte[3309], DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(), null));
            var unitOfWork = new FakeUnitOfWork();
            var identities = new UserIdentityBundleService(devices, lists, users, new FakeFriendshipRepository(areFriends),
                new NoSharedRooms(), new FakeAccessOverrideRepository(), new NoExposures(), Lifecycle, new FakeRoomLifecycleLock(), unitOfWork);
            Service = new ArtifactEndorsementService(Rows, users, devices, lists, Signatures, Lifecycle, unitOfWork, identities, TimeProvider.System);
        }

        public Task PutAsync(string digest) => Service.PutAsync(User.Id, User.IdentityIncarnationId!.Value, digest, Device.DeviceId, new byte[3309], TestContext.Current.CancellationToken);
    }

    private sealed class RecordingSignatures : IPopSignatureVerifier
    {
        public bool Accept { get; set; } = true;
        public bool Verify(ReadOnlySpan<byte> publicKey, ReadOnlySpan<byte> message, ReadOnlySpan<byte> signature) => Accept;
    }

    private sealed class RecordingLifecycle : IUserLifecycleLock
    {
        public int Calls { get; private set; }
        public Task AcquireAsync(UserId userId, CancellationToken ct = default) { Calls++; return Task.CompletedTask; }
    }

    private sealed class MemoryEndorsements : IArtifactEndorsementRepository
    {
        public List<ArtifactEndorsement> Rows { get; } = [];
        public int? CountOverride { get; set; }
        public Task<ArtifactEndorsement?> GetAsync(UserId userId, Guid incarnationId, string digest, CancellationToken ct = default) =>
            Task.FromResult(Rows.SingleOrDefault(row => row.UserId == userId && row.IdentityIncarnationId == incarnationId && row.ArtifactDigest == digest));
        public Task<IReadOnlyList<ArtifactEndorsement>> ListAsync(UserId userId, Guid incarnationId, int offset, int limit, CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<ArtifactEndorsement>>(Rows.Where(row => row.UserId == userId && row.IdentityIncarnationId == incarnationId).OrderBy(row => row.ArtifactDigest, StringComparer.Ordinal).Skip(offset).Take(limit).ToList());
        public Task<int> CountAsync(UserId userId, Guid incarnationId, CancellationToken ct = default) =>
            Task.FromResult(CountOverride ?? Rows.Count(row => row.UserId == userId && row.IdentityIncarnationId == incarnationId));
        public Task DeleteOtherIncarnationsAsync(UserId userId, Guid incarnationId, CancellationToken ct = default)
        { Rows.RemoveAll(row => row.UserId == userId && row.IdentityIncarnationId != incarnationId); return Task.CompletedTask; }
        public Task AddAsync(ArtifactEndorsement endorsement, CancellationToken ct = default)
        { Rows.Add(endorsement); return Task.CompletedTask; }
    }

    private sealed class NoSharedRooms : ISharedRoomAuthorizationRepository
    {
        public Task<IReadOnlyList<RoomId>> GetSharedActiveRoomIdsAsync(UserId firstUserId, UserId secondUserId, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<RoomId>>([]);
        public Task<IReadOnlyList<RoomId>> GetHistoricalArtifactRoomIdsAsync(UserId readerUserId, UserId authorUserId, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<RoomId>>([]);
    }

    private sealed class NoExposures : IIdentityExposureRepository
    {
        public Task RecordAsync(UserId identityOwnerUserId, UserId recipientUserId, DateTimeOffset exposedAt, CancellationToken ct = default) => Task.CompletedTask;
        public Task<IReadOnlyList<UserId>> GetHistoricalPeerUserIdsAsync(UserId userId, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<UserId>>([]);
        public Task<IReadOnlyList<IdentityLifecycleProjection>> GetLifecycleSnapshotForRecipientAsync(UserId recipientUserId, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<IdentityLifecycleProjection>>([]);
    }
}
