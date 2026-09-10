using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class DeviceWorkflowServiceTests
{
    [Fact]
    public async Task Invalid_Pop_Commits_Only_Challenge_Burn()
    {
        var fixture = CreateEnrollmentFixture(popValid: false);

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.EnrollAsync(fixture.Command, TestContext.Current.CancellationToken));

        Assert.True(fixture.Challenges.Consumed);
        Assert.Equal(["begin", "commit", "dispose"], fixture.UnitOfWork.Events);
        Assert.Equal(0, fixture.UnitOfWork.SaveCalls);
        Assert.Empty(fixture.Devices.Added);
    }

    [Fact]
    public async Task Post_Burn_Failure_Rolls_Back_To_Savepoint_And_Commits_Burn()
    {
        var fixture = CreateEnrollmentFixture(verifierFailure: true);

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            fixture.Service.EnrollAsync(fixture.Command, TestContext.Current.CancellationToken));

        Assert.True(fixture.Challenges.Consumed);
        Assert.Equal(
            ["begin", "savepoint:challenge_consumed", "rollback:challenge_consumed", "commit", "dispose"],
            fixture.UnitOfWork.Events);
        Assert.Equal(0, fixture.UnitOfWork.SaveCalls);
    }

    [Fact]
    public async Task Successful_Enrollment_Saves_Then_Commits_Once()
    {
        var certificateExpiresAt = DateTimeOffset.FromUnixTimeMilliseconds(1_500_086_400_000);
        var fixture = CreateEnrollmentFixture(certificateExpiresAt: certificateExpiresAt);

        var result = await fixture.Service.EnrollAsync(
            fixture.Command,
            TestContext.Current.CancellationToken);

        Assert.True(result.BootstrappedIdentity);
        Assert.NotNull(result.IdentityIncarnationId);
        Assert.Equal(1, result.IdentityRevision);
        Assert.Equal(
            ["begin", "savepoint:challenge_consumed", "save", "commit", "dispose"],
            fixture.UnitOfWork.Events);
        var enrolledDevice = Assert.Single(fixture.Devices.Added);
        Assert.Equal(certificateExpiresAt, enrolledDevice.CertExpiresAt);
        var invalidation = Assert.Single(fixture.Links.InvalidationCalls);
        Assert.Equal(fixture.Command.UserId, invalidation.UserId);
        Assert.Equal(1, invalidation.CommittedGeneration);
        Assert.Null(invalidation.ExcludingRequestId);
        Assert.Equal(CancellationToken.None, fixture.UnitOfWork.CommitToken);
    }

    [Fact]
    public async Task Enrollment_Replays_Exact_Committed_Bundle_Without_Reusing_Challenge()
    {
        var fixture = CreateEnrollmentFixture();
        var first = await fixture.Service.EnrollAsync(
            fixture.Command,
            TestContext.Current.CancellationToken);
        var replacementChallenge = new ChallengeRepository();
        var replayUnitOfWork = new RecordingUnitOfWork();
        var replay = new DeviceEnrollmentService(
            fixture.Devices,
            fixture.DeviceLists,
            fixture.Links,
            fixture.Users,
            new PopChallengeConsumer(
                replacementChallenge,
                new ResultSignatureVerifier(false),
                new NoOpAuthMetrics()),
            fixture.Verifier,
            new EnrollmentListParser(fixture.Command.UserId, 1),
            new FakeUserLifecycleLock(),
            replayUnitOfWork);

        var result = await replay.EnrollAsync(
            fixture.Command,
            TestContext.Current.CancellationToken);

        Assert.Equal(first.DeviceId, result.DeviceId);
        Assert.Equal(first.DeviceListGeneration, result.DeviceListGeneration);
        Assert.False(replacementChallenge.Consumed);
        Assert.Equal(["begin", "commit", "dispose"], replayUnitOfWork.Events);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task Enrollment_Replay_Rejects_Request_Key_Mismatch(bool alterSigningKey)
    {
        var fixture = CreateEnrollmentFixture();
        _ = await fixture.Service.EnrollAsync(
            fixture.Command,
            TestContext.Current.CancellationToken);
        var replacementChallenge = new ChallengeRepository();
        var replay = new DeviceEnrollmentService(
            fixture.Devices,
            fixture.DeviceLists,
            fixture.Links,
            fixture.Users,
            new PopChallengeConsumer(
                replacementChallenge,
                new ResultSignatureVerifier(false),
                new NoOpAuthMetrics()),
            fixture.Verifier,
            new EnrollmentListParser(fixture.Command.UserId, 1),
            new FakeUserLifecycleLock(),
            new RecordingUnitOfWork());
        var mismatched = fixture.Command with
        {
            KemPublicKey = alterSigningKey
                ? fixture.Command.KemPublicKey
                : Enumerable.Repeat((byte)1, 1184).ToArray(),
            SigningPublicKey = alterSigningKey
                ? Enumerable.Repeat((byte)1, 1952).ToArray()
                : fixture.Command.SigningPublicKey,
        };

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            replay.EnrollAsync(mismatched, TestContext.Current.CancellationToken));

        Assert.False(replacementChallenge.Consumed);
    }

    [Fact]
    public async Task Enrollment_Replays_Historical_Bundle_After_Later_List_Generation()
    {
        var fixture = CreateEnrollmentFixture();
        _ = await fixture.Service.EnrollAsync(
            fixture.Command,
            TestContext.Current.CancellationToken);
        await fixture.DeviceLists.AddAsync(TestDeviceList.Create(
            fixture.Command.UserId,
            2,
            TestDeviceList.Entries([("device-1", "device-1")]),
            "device-1",
            TestDeviceCertificate.Signature(10),
            2,
            null),
            TestContext.Current.CancellationToken);
        var replay = new DeviceEnrollmentService(
            fixture.Devices,
            fixture.DeviceLists,
            fixture.Links,
            fixture.Users,
            new PopChallengeConsumer(
                new ChallengeRepository(),
                new ResultSignatureVerifier(false),
                new NoOpAuthMetrics()),
            fixture.Verifier,
            new EnrollmentListParser(fixture.Command.UserId, 1),
            new FakeUserLifecycleLock(),
            new RecordingUnitOfWork());

        var result = await replay.EnrollAsync(
            fixture.Command,
            TestContext.Current.CancellationToken);

        Assert.Equal(2, result.DeviceListGeneration);
        Assert.True(result.BootstrappedIdentity);
    }

    [Fact]
    public async Task Ambiguous_Final_Commit_Is_Not_Rolled_Back_Or_Retried()
    {
        var fixture = CreateEnrollmentFixture(throwAfterCommit: true);

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            fixture.Service.EnrollAsync(fixture.Command, TestContext.Current.CancellationToken));

        Assert.Equal(
            ["begin", "savepoint:challenge_consumed", "save", "commit", "dispose"],
            fixture.UnitOfWork.Events);
        Assert.DoesNotContain(fixture.UnitOfWork.Events, entry => entry.StartsWith("rollback:"));
        Assert.Equal(1, fixture.UnitOfWork.CommitCalls);
    }

    [Fact]
    public async Task Link_Approval_Invalidates_Older_Receipt_But_Excludes_Current_Request()
    {
        var userId = UserId.New();
        var user = User.Create(
            userId,
            $"{userId.Value:N}@example.test",
            "approval-user",
            "Approval user");
        user.AdvanceIdentityLifecycle(Guid.CreateVersion7());
        var users = new FakeUserRepository(user);
        var devices = new RecordingDeviceRepository();
        var lists = new EnrollmentListRepository(TestDeviceList.Create(
            userId,
            1,
            TestDeviceList.Entries([("signer-device", "signer-device")]),
            "signer-device",
            [2],
            1,
            null));
        var links = new LinkRepository();
        var older = DeviceLinkRequest.Create(
            userId,
            "older-device-code",
            "BCDF-2345",
            "older-device",
            "Older device",
            new byte[1184],
            new byte[1952],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
        older.Approve(1, DateTimeOffset.UtcNow);
        var current = DeviceLinkRequest.Create(
            userId,
            "current-device-code",
            "GHJK-6789",
            "current-device",
            "Current device",
            new byte[1184],
            new byte[1952],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
        links.RequestsInternal.AddRange([older, current]);
        var realtime = new RecordingDeviceLinkRealtimeEffects();
        var certificateExpiresAt = DateTimeOffset.FromUnixTimeMilliseconds(1_500_086_400_000);
        var service = new DeviceLinkApprovalService(
            links,
            devices,
            lists,
            users,
            new EnrollmentVerifier(
                userId,
                current.DeviceId,
                failure: false,
                generation: 2,
                certificateExpiresAt: certificateExpiresAt),
            new FakeUserLifecycleLock(),
            new RecordingUnitOfWork(),
            realtime);

        var approvedListBody = TestDeviceList.Body(
            userId,
            2,
            TestDeviceList.Entries([
                ("signer-device", "signer-device"),
                (current.DeviceId, "signer-device"),
            ]),
            "signer-device",
            certificateExpiresAt.AddDays(-1).ToUnixTimeMilliseconds(),
            null);
        var certificate = TestDeviceCertificate.Body(
            userId,
            current.DeviceId,
            "Laptop",
            "signer-device",
            current.KemPublicKey,
            current.SigningPublicKey,
            certificateExpiresAt.AddDays(-1).ToUnixTimeMilliseconds(),
            certificateExpiresAt.ToUnixTimeMilliseconds());
        Assert.True(await service.ApproveAsync(
            userId,
            current.UserCode,
            certificate,
            TestDeviceCertificate.Signature(4),
            approvedListBody,
            TestDeviceCertificate.Signature(6),
            TestContext.Current.CancellationToken));

        var invalidation = Assert.Single(links.InvalidationCalls);
        Assert.Equal(userId, invalidation.UserId);
        Assert.Equal(2, invalidation.CommittedGeneration);
        Assert.Equal(current.Id, invalidation.ExcludingRequestId);
        Assert.NotNull(older.CancelledAt);
        Assert.Null(current.CancelledAt);
        Assert.Equal(2, current.DeviceListGeneration);
        Assert.Equal(certificateExpiresAt, Assert.Single(devices.Added).CertExpiresAt);
        Assert.Equal((userId, 2L, current.UserCode), Assert.Single(realtime.Publications));
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Device_List_Replacement_Invalidates_Older_Approved_Receipts(bool revokeDevice)
    {
        var userId = UserId.New();
        var signer = TestDeviceCertificate.CreateDevice(
            userId,
            "signer-device",
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Signer",
            signerDeviceId: "signer-device",
            issuedAt: DateTimeOffset.FromUnixTimeMilliseconds(1),
            expiresAt: null);
        var revoked = TestDeviceCertificate.CreateDevice(
            userId, "revoked-device", new byte[1184], new byte[1952], "Revoked",
            "signer-device", DateTimeOffset.FromUnixTimeMilliseconds(1), null);
        var currentList = TestDeviceList.Create(
            userId,
            1,
            TestDeviceList.Entries(revokeDevice
                ? [("signer-device", "signer-device"), ("revoked-device", "signer-device")]
                : [("signer-device", "signer-device")]),
            "signer-device",
            [2],
            1,
            null);
        var user = User.Create(
            userId,
            $"{userId.Value:N}@example.test",
            "replacement-user",
            "Replacement user");
        user.AdvanceIdentityLifecycle(Guid.CreateVersion7());
        var links = new LinkRepository();
        var older = DeviceLinkRequest.Create(
            userId,
            "older-replacement-code",
            "MNPQ-2345",
            "older-replacement-device",
            "Older replacement device",
            new byte[1184],
            new byte[1952],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
        older.Approve(1, DateTimeOffset.UtcNow);
        links.RequestsInternal.Add(older);
        var unitOfWork = new RecordingUnitOfWork();
        var effects = new ImmediateDeviceListRealtimeEffects(unitOfWork);
        var invitee = UserId.New();
        var invitationCancellation = new RecordingRevokedInvitations(unitOfWork, invitee);
        var service = new DeviceListReplacementService(
            new FakeUserDeviceRepository(revokeDevice ? [signer, revoked] : [signer]),
            new FakeUserDeviceListRepository(currentList),
            links,
            invitationCancellation,
            new FakeUserRepository(user),
            new EmptyRevocationKeyBlobs(),
            new EmptySemanticRelayLifecycleRepository(),
            new FakeFriendshipRepository(),
            new FakeRoomMemberRepository(),
            new EmptyIdentityExposureRepository(),
            new EmptyDeviceRevocationAuditRepository(),
            new ReplacementListVerifier(userId),
            new FakeUserLifecycleLock(),
            unitOfWork,
            effects,
            new EmptyDeviceRevocationDurabilityCoordinator(),
            new NoOpAuthMetrics(),
            new TestTimeProvider(DateTimeOffset.FromUnixTimeMilliseconds(2)));

        var replacementBody = TestDeviceList.Body(
            userId,
            2,
            TestDeviceList.Entries([("signer-device", "signer-device")]),
            "signer-device",
            2,
            null);
        await service.ReplaceAsync(
            userId,
            replacementBody,
            TestDeviceCertificate.Signature(4),
            new RequestAuditContext(null, null),
            TestContext.Current.CancellationToken);

        var invalidation = Assert.Single(links.InvalidationCalls);
        Assert.Equal(userId, invalidation.UserId);
        Assert.Equal(2, invalidation.CommittedGeneration);
        Assert.Null(invalidation.ExcludingRequestId);
        Assert.NotNull(older.CancelledAt);
        Assert.Equal(revokeDevice ? 1 : 0, effects.PersistCalls);
        Assert.Equal(revokeDevice ? 1 : 0, invitationCancellation.Calls);
        if (revokeDevice)
        {
            Assert.NotNull(revoked.RevokedAt);
            Assert.Equal(new[] { invitee, userId }, effects.InvitationAudience);
        }
        else
        {
            Assert.Empty(effects.InvitationAudience);
        }
    }

    [Fact]
    public async Task Challenge_Creation_Persists_Exact_Thirty_Two_Byte_Challenge()
    {
        var repository = new ChallengeRepository();
        var unitOfWork = new RecordingUnitOfWork();
        var result = await new DeviceRegistrationChallengeService(repository, unitOfWork)
            .CreateAsync(UserId.New(), TestContext.Current.CancellationToken);

        Assert.Equal(32, result.Challenge.Length);
        Assert.Equal(result.Challenge, Assert.Single(repository.Added).Challenge);
        Assert.Equal(1, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task Link_Initiation_Replays_Exact_Pending_Request_After_Ambiguous_Commit()
    {
        var userId = UserId.New();
        var repository = new LinkRepository();
        var firstUnitOfWork = new RecordingUnitOfWork(throwAfterCommit: true);
        var service = new DeviceLinkRequestService(
            repository,
            new FakeUserLifecycleLock(),
            firstUnitOfWork,
            TimeProvider.System);

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.InitiateAsync(
                userId,
                "device-1",
                "Laptop",
                new byte[1184],
                new byte[1952],
                TestContext.Current.CancellationToken));
        var committed = Assert.Single(repository.Requests);

        var replay = await new DeviceLinkRequestService(
            repository,
            new FakeUserLifecycleLock(),
            new RecordingUnitOfWork(),
            TimeProvider.System).InitiateAsync(
                userId,
                "device-1",
                "Laptop",
                new byte[1184],
                new byte[1952],
                TestContext.Current.CancellationToken);

        Assert.Equal(committed.DeviceCode, replay.DeviceCode);
        Assert.Equal(committed.UserCode, replay.UserCode);
        Assert.Single(repository.Requests);
    }

    [Fact]
    public async Task Link_Request_Rejects_More_Than_Three_Pending_Requests()
    {
        var userId = UserId.New();
        var repository = new LinkRepository();
        var service = new DeviceLinkRequestService(
            repository,
            new FakeUserLifecycleLock(),
            new RecordingUnitOfWork(),
            TimeProvider.System);
        for (var index = 0; index < 3; index++)
        {
            _ = await service.InitiateAsync(
                userId,
                $"device-{index}",
                "Laptop",
                new byte[1184],
                new byte[1952],
                TestContext.Current.CancellationToken);
        }

        await Assert.ThrowsAsync<DeviceEnrollmentException>(() =>
            service.InitiateAsync(
                userId,
                "device-4",
                "Laptop",
                new byte[1184],
                new byte[1952],
                TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Link_Cancellation_Normalizes_Code_And_Serializes_Atomic_Transition()
    {
        var userId = UserId.New();
        var events = new List<string>();
        var repository = new LinkRepository(events);
        var unitOfWork = new RecordingUnitOfWork(events: events);
        var service = new DeviceLinkRequestService(
            repository,
            new RecordingUserLifecycleLock(events),
            unitOfWork,
            TimeProvider.System);
        var initiated = await service.InitiateAsync(
            userId,
            "device-1",
            "Laptop",
            new byte[1184],
            new byte[1952],
            TestContext.Current.CancellationToken);

        var cancelled = await service.CancelAsync(
            userId,
            $" {initiated.UserCode.ToLowerInvariant()} ",
            TestContext.Current.CancellationToken);

        Assert.Equal(initiated.UserCode, cancelled?.UserCode);
        Assert.Equal(1, unitOfWork.SaveCalls);
        Assert.Equal(2, unitOfWork.CommitCalls);
        Assert.Equal(CancellationToken.None, unitOfWork.CommitToken);
        Assert.Equal(
            [
                "begin",
                "lock",
                "savepoint:device_link_code_allocation",
                "save",
                "commit",
                "dispose",
                "begin",
                "lock",
                "cancel",
                "commit",
                "dispose",
            ],
            events);
    }

    [Fact]
    public async Task Link_Cancellation_Maps_Approved_And_Expired_As_Conflicts()
    {
        var userId = UserId.New();
        var repository = new LinkRepository();
        var service = new DeviceLinkRequestService(
            repository,
            new FakeUserLifecycleLock(),
            new RecordingUnitOfWork(),
            TimeProvider.System);
        var approved = DeviceLinkRequest.Create(
            userId,
            "approved-device-code",
            "BCDF-2345",
            "approved-device",
            "Laptop",
            new byte[1184],
            new byte[1952],
            TimeSpan.FromMinutes(15), DateTimeOffset.UtcNow);
        approved.Approve(2, DateTimeOffset.UtcNow);
        repository.RequestsInternal.Add(approved);
        var expired = DeviceLinkRequest.Create(
            userId,
            "expired-device-code",
            "GHJK-6789",
            "expired-device",
            "Tablet",
            new byte[1184],
            new byte[1952],
            TimeSpan.FromTicks(1), DateTimeOffset.UtcNow);
        repository.RequestsInternal.Add(expired);
        await Task.Delay(5, TestContext.Current.CancellationToken);

        await Assert.ThrowsAsync<ConflictException>(() => service.CancelAsync(
            userId,
            approved.UserCode,
            TestContext.Current.CancellationToken));
        await Assert.ThrowsAsync<ConflictException>(() => service.CancelAsync(
            userId,
            expired.UserCode,
            TestContext.Current.CancellationToken));
        Assert.Null(await service.CancelAsync(
            UserId.New(),
            approved.UserCode,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Link_Request_Conceals_Foreign_Pending_And_Acknowledgement()
    {
        var owner = UserId.New();
        var repository = new LinkRepository();
        var service = new DeviceLinkRequestService(
            repository,
            new FakeUserLifecycleLock(),
            new RecordingUnitOfWork(),
            TimeProvider.System);
        var initiated = await service.InitiateAsync(
            owner,
            "device-1",
            "Laptop",
            new byte[1184],
            new byte[1952],
            TestContext.Current.CancellationToken);

        Assert.Null(await service.GetPendingAsync(
            UserId.New(),
            initiated.UserCode,
            TestContext.Current.CancellationToken));
        Assert.Equal(
            DeviceLinkAcknowledgeResult.NotFound,
            await service.AcknowledgeAsync(
                UserId.New(),
                initiated.DeviceCode,
                "device-1",
                TestContext.Current.CancellationToken));
    }

    private static EnrollmentFixture CreateEnrollmentFixture(
        bool popValid = true,
        bool verifierFailure = false,
        bool throwAfterCommit = false,
        DateTimeOffset? certificateExpiresAt = null)
    {
        var userId = UserId.New();
        var challenge = DeviceRegistrationChallenge.Create(
            userId,
            new byte[32],
            TimeSpan.FromMinutes(5));
        var challenges = new ChallengeRepository(challenge);
        var devices = new RecordingDeviceRepository();
        var lists = new EnrollmentListRepository();
        var links = new LinkRepository();
        var user = User.Create(
            userId,
            $"{userId.Value:N}@example.test",
            $"user-{userId.Value:N}",
            "User");
        var users = new FakeUserRepository(user);
        var unitOfWork = new RecordingUnitOfWork(throwAfterCommit);
        const string deviceId = "device-1";
        var signedDeviceList = TestDeviceList.Body(
            userId,
            1,
            TestDeviceList.Entries([(deviceId, deviceId)]),
            deviceId,
            1,
            null);
        var kemPublicKey = new byte[1184];
        var signingPublicKey = new byte[1952];
        var certificateIssuedAt = certificateExpiresAt?.AddDays(-1)
            ?? DateTimeOffset.UtcNow.AddMinutes(-1);
        var certificate = TestDeviceCertificate.Body(
            userId,
            deviceId,
            "Laptop",
            deviceId,
            kemPublicKey,
            signingPublicKey,
            certificateIssuedAt.ToUnixTimeMilliseconds(),
            certificateExpiresAt?.ToUnixTimeMilliseconds());
        var command = new DeviceEnrollmentCommand(
            userId,
            deviceId,
            kemPublicKey,
            signingPublicKey,
            challenge.Id,
            [1],
            certificate,
            TestDeviceCertificate.Signature(3),
            signedDeviceList,
            TestDeviceCertificate.Signature(5));
        var verifier = new EnrollmentVerifier(
            userId,
            deviceId,
            verifierFailure,
            certificateExpiresAt: certificateExpiresAt);
        var service = new DeviceEnrollmentService(
            devices,
            lists,
            links,
            users,
            new PopChallengeConsumer(
                challenges,
                new ResultSignatureVerifier(popValid),
                new NoOpAuthMetrics()),
            verifier,
            new EnrollmentListParser(userId, 1),
            new FakeUserLifecycleLock(),
            unitOfWork);
        return new EnrollmentFixture(
            service,
            command,
            challenges,
            devices,
            lists,
            links,
            users,
            verifier,
            unitOfWork);
    }

    private sealed record EnrollmentFixture(
        DeviceEnrollmentService Service,
        DeviceEnrollmentCommand Command,
        ChallengeRepository Challenges,
        RecordingDeviceRepository Devices,
        EnrollmentListRepository DeviceLists,
        LinkRepository Links,
        FakeUserRepository Users,
        EnrollmentVerifier Verifier,
        RecordingUnitOfWork UnitOfWork);

    private sealed class ChallengeRepository(params DeviceRegistrationChallenge[] challenges)
        : IDeviceRegistrationChallengeRepository
    {
        private readonly Dictionary<Guid, DeviceRegistrationChallenge> _challenges =
            challenges.ToDictionary(challenge => challenge.Id);

        public List<DeviceRegistrationChallenge> Added { get; } = [];
        public bool Consumed { get; private set; }

        public Task AddAsync(
            DeviceRegistrationChallenge challenge,
            CancellationToken ct = default)
        {
            Added.Add(challenge);
            _challenges[challenge.Id] = challenge;
            return Task.CompletedTask;
        }

        public Task<DeviceRegistrationChallenge?> GetByIdAsync(
            Guid id,
            CancellationToken ct = default) =>
            Task.FromResult(_challenges.GetValueOrDefault(id));

        public Task<bool> TryConsumeAsync(
            DeviceRegistrationChallenge challenge,
            CancellationToken ct = default)
        {
            if (!_challenges.TryGetValue(challenge.Id, out var current)
                || !ReferenceEquals(current, challenge)
                || !challenge.IsValid(DateTimeOffset.UtcNow))
            {
                return Task.FromResult(false);
            }

            _challenges.Remove(challenge.Id);
            Consumed = true;
            return Task.FromResult(true);
        }

        public Task<int> DeleteExpiredAsync(
            DateTimeOffset now,
            int limit,
            CancellationToken ct = default) => throw new NotSupportedException();
    }

    private sealed class RecordingDeviceRepository : IUserDeviceRepository
    {
        public List<UserDevice> Added { get; } = [];

        public Task<UserDevice?> GetByDeviceIdAsync(
            string deviceId,
            CancellationToken ct = default) =>
            Task.FromResult<UserDevice?>(Added.SingleOrDefault(device =>
                device.DeviceId == deviceId));

        public Task AddAsync(UserDevice device, CancellationToken ct = default)
        {
            Added.Add(device);
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<UserDevice>> GetByUserIdAsync(
            UserId userId,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<IReadOnlyDictionary<string, UserId>> GetUserIdsByDeviceIdsAsync(
            IReadOnlyCollection<string> deviceIds,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<IReadOnlyList<UserDevice>> GetByUserIdsAsync(
            IReadOnlyList<UserId> userIds,
            CancellationToken ct = default) => throw new NotSupportedException();

        public void Update(UserDevice device) => throw new NotSupportedException();

        public Task<int> RemoveAllForUserAsync(
            UserId userId,
            CancellationToken ct = default) => throw new NotSupportedException();
    }

    private sealed class EnrollmentListRepository(params UserDeviceList[] lists)
        : IUserDeviceListRepository
    {
        private readonly List<UserDeviceList> _lists = [.. lists];

        public Task<UserDeviceList?> GetLatestAsync(
            UserId userId,
            CancellationToken ct = default) =>
            Task.FromResult(_lists
                .Where(list => list.UserId == userId)
                .MaxBy(list => list.Generation));

        public Task<UserDeviceList?> GetGenerationAsync(
            UserId userId,
            long generation,
            CancellationToken ct = default) =>
            Task.FromResult(_lists.SingleOrDefault(list =>
                list.UserId == userId && list.Generation == generation));

        public Task AddAsync(UserDeviceList list, CancellationToken ct = default)
        {
            _lists.Add(list);
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<UserDeviceList>> GetLatestByUserIdsAsync(
            IReadOnlyCollection<UserId> userIds,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<int> RemoveAllForUserAsync(
            UserId userId,
            CancellationToken ct = default) => throw new NotSupportedException();
    }

    private sealed class EnrollmentListParser(UserId userId, long generation)
        : ISignedDeviceListVerifier
    {
        public SignedDeviceListParser.ParsedSignedDeviceList Parse(ReadOnlySpan<byte> body) => new(
            userId.Value.ToString(),
            generation,
            [],
            "device-1",
            1,
            null);

        public bool Verify(
            ReadOnlySpan<byte> body,
            ReadOnlySpan<byte> signature,
            ReadOnlySpan<byte> signerPublicKey) => true;
    }

    private sealed class EnrollmentVerifier(
        UserId userId,
        string deviceId,
        bool failure,
        long generation = 1,
        DateTimeOffset? certificateExpiresAt = null) : IDeviceEnrollmentVerifier
    {
        public Task<VerifiedDeviceLinkEnrollment> VerifyAsync(
            UserId requestedUserId,
            string requestedDeviceId,
            byte[] kemPublicKey,
            byte[] signingPublicKey,
            byte[] certificate,
            byte[] certificateSignature,
            byte[] signedDeviceList,
            byte[] signedDeviceListSignature,
            CancellationToken ct = default)
        {
            if (failure)
            {
                throw new DeviceEnrollmentException("verification failed");
            }
            Assert.Equal(userId, requestedUserId);
            Assert.Equal(deviceId, requestedDeviceId);
            var verifiedAt = certificateExpiresAt?.AddDays(-1) ?? DateTimeOffset.UtcNow;
            return Task.FromResult(new VerifiedDeviceLinkEnrollment(
                new SignedDeviceListParser.ParsedSignedDeviceList(
                    userId.Value.ToString(),
                    generation,
                    [new SignedDeviceListParser.ListEntry(deviceId, deviceId)],
                    deviceId,
                    verifiedAt.ToUnixTimeMilliseconds(),
                    null),
                verifiedAt));
        }
    }

    private sealed class ResultSignatureVerifier(bool result) : IPopSignatureVerifier
    {
        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature) => result;
    }

    private sealed class NoOpAuthMetrics : IAuthMetrics
    {
        public void RecordPopFailure(PopFailureReason reason, string endpoint) { }
    }

    private sealed class ReplacementListVerifier(UserId userId) : ISignedDeviceListVerifier
    {
        public SignedDeviceListParser.ParsedSignedDeviceList Parse(ReadOnlySpan<byte> body) => new(
            userId.Value.ToString(),
            2,
            [new SignedDeviceListParser.ListEntry("signer-device", "signer-device")],
            "signer-device",
            2,
            null);

        public bool Verify(
            ReadOnlySpan<byte> body,
            ReadOnlySpan<byte> signature,
            ReadOnlySpan<byte> signerPublicKey) => true;
    }

    private sealed class EmptyRevocationKeyBlobs : ISessionKeyBlobRepository
    {
        public Task<SessionKeyBlob?> GetForDeviceAsync(SessionId sessionId, string recipientDeviceId, CancellationToken ct = default) => throw new NotSupportedException();
        public Task AddRangeAsync(IReadOnlyList<SessionKeyBlob> blobs, CancellationToken ct = default) => throw new NotSupportedException();
        public Task DeleteForSessionAsync(SessionId sessionId, CancellationToken ct = default) => throw new NotSupportedException();
        public Task<IReadOnlyList<DeviceRevocationSessionTarget>> GetSessionTargetsForRecipientDevicesAsync(IReadOnlyCollection<string> recipientDeviceIds, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<DeviceRevocationSessionTarget>>([]);
        public Task<IReadOnlyList<SessionId>> GetSessionIdsForRecipientDevicesAsync(IReadOnlyCollection<string> recipientDeviceIds, CancellationToken ct = default) => throw new NotSupportedException();
        public Task<int> DeleteForRecipientDevicesAsync(IReadOnlyCollection<string> recipientDeviceIds, CancellationToken ct = default) => Task.FromResult(0);
    }

    private sealed class EmptySemanticRelayLifecycleRepository
        : ISemanticRelayLifecycleRepository
    {
        public Task DeleteForAccountAsync(UserId userId, CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task DeleteForDevicesAsync(
            UserId userId,
            IReadOnlyCollection<string> deviceIds,
            CancellationToken ct = default) => Task.CompletedTask;
    }

    private sealed class EmptyIdentityExposureRepository : IIdentityExposureRepository
    {
        public Task RecordAsync(
            UserId identityOwnerUserId,
            UserId recipientUserId,
            DateTimeOffset exposedAt,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<UserId>> GetHistoricalPeerUserIdsAsync(
            UserId userId,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<UserId>>([]);

        public Task<IReadOnlyList<IdentityLifecycleProjection>>
            GetLifecycleSnapshotForRecipientAsync(
                UserId recipientUserId,
                CancellationToken ct = default) =>
            throw new NotSupportedException();
    }

    private sealed class EmptyDeviceRevocationAuditRepository
        : IDeviceRevocationAuditRepository
    {
        public Task AddAsync(
            DeviceRevocationAuditEntry entry,
            CancellationToken ct = default) => Task.CompletedTask;
    }

    private sealed class RecordingRevokedInvitations(RecordingUnitOfWork unitOfWork, UserId invitee) : IRevokedDeviceInvitationCancellation
    {
        public int Calls { get; private set; }
        public Task<IReadOnlyList<UserId>> CancelPendingAsync(UserId ownerUserId, IReadOnlyCollection<string> revokedDeviceIds, DateTimeOffset cancelledAt, CancellationToken ct = default)
        {
            Assert.Equal(0, unitOfWork.CommitCalls);
            Assert.Contains("revoked-device", revokedDeviceIds);
            Calls++;
            return Task.FromResult<IReadOnlyList<UserId>>([invitee]);
        }
    }

    private sealed class ImmediateDeviceListRealtimeEffects(RecordingUnitOfWork unitOfWork) : IDeviceListRealtimeEffects
    {
        public int PersistCalls { get; private set; }
        public IReadOnlyCollection<UserId> InvitationAudience { get; private set; } = [];

        public async Task PersistAndEnforceAsync(
            UserId userId,
            IReadOnlyCollection<string> revokedDeviceIds,
            IReadOnlyCollection<DeviceRevocationSessionTarget> sessionsWithRevokedKeys,
            Func<CancellationToken, Task> persistAndCommitAsync,
            CancellationToken ct = default)
        {
            PersistCalls++;
            await persistAndCommitAsync(ct);
        }

        public Task EnforceCommittedAsync(
            UserId userId,
            IReadOnlyCollection<string> revokedDeviceIds,
            IReadOnlyCollection<DeviceRevocationSessionTarget> sessionsWithRevokedKeys,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public void ReportEnforcementPending(Guid revocationId, Exception exception) =>
            throw new NotSupportedException();

        public void PublishInvitationsChanged(IReadOnlyCollection<UserId> audience)
        {
            Assert.Equal(1, unitOfWork.CommitCalls);
            InvitationAudience = audience;
        }

        public void PublishChanged(
            IReadOnlyCollection<UserId> audience,
            UserId userId,
            long generation)
        {
        }
    }

    private sealed class EmptyDeviceRevocationDurabilityCoordinator
        : IDeviceRevocationDurabilityCoordinator
    {
        public Task<IReadOnlyList<DeviceRevocationEnforcementWork>> GetPendingEnforcementAsync(
            int limit,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<bool> ExecuteIfCurrentAsync(
            DeviceRevocationEnforcementWork work,
            Func<CancellationToken, Task> enforce,
            CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task CompleteEnforcementAsync(
            Guid revocationId,
            CancellationToken ct = default) => Task.CompletedTask;
    }

    private sealed class RecordingDeviceLinkRealtimeEffects : IDeviceLinkRealtimeEffects
    {
        public List<(UserId UserId, long Generation, string UserCode)> Publications { get; } = [];

        public Task PublishApprovedAsync(
            UserId userId,
            long generation,
            string userCode)
        {
            Publications.Add((userId, generation, userCode));
            return Task.CompletedTask;
        }
    }

    private sealed class RecordingUserLifecycleLock(List<string> events)
        : IUserLifecycleLock
    {
        public Task AcquireAsync(UserId userId, CancellationToken ct = default)
        {
            events.Add("lock");
            return Task.CompletedTask;
        }
    }

    private sealed class RecordingUnitOfWork(
        bool throwAfterCommit = false,
        List<string>? events = null) : UnitOfWorkStub
    {
        private readonly bool _throwAfterCommit = throwAfterCommit;

        public List<string> Events { get; } = events ?? [];
        public int SaveCalls { get; private set; }
        public int CommitCalls { get; private set; }
        public CancellationToken CommitToken { get; private set; }

        public override Task SaveChangesAsync(CancellationToken ct = default)
        {
            SaveCalls++;
            Events.Add("save");
            return Task.CompletedTask;
        }

        public override Task<ITransactionScope> BeginTransactionAsync(
            CancellationToken ct = default)
        {
            Events.Add("begin");
            return Task.FromResult<ITransactionScope>(new Transaction(this));
        }

        private sealed class Transaction(RecordingUnitOfWork owner) : TransactionScopeStub
        {
            public override Task CreateSavepointAsync(
                string name,
                CancellationToken ct = default)
            {
                owner.Events.Add($"savepoint:{name}");
                return Task.CompletedTask;
            }

            public override Task RollbackToSavepointAsync(
                string name,
                CancellationToken ct = default)
            {
                owner.Events.Add($"rollback:{name}");
                return Task.CompletedTask;
            }

            public override Task CommitAsync(CancellationToken ct = default)
            {
                owner.CommitCalls++;
                owner.CommitToken = ct;
                owner.Events.Add("commit");
                return owner._throwAfterCommit
                    ? Task.FromException(new InvalidOperationException("ambiguous commit"))
                    : Task.CompletedTask;
            }

            public override ValueTask DisposeAsync()
            {
                owner.Events.Add("dispose");
                return ValueTask.CompletedTask;
            }
        }
    }

    private sealed record InvalidationCall(
        UserId UserId,
        long CommittedGeneration,
        Guid? ExcludingRequestId,
        DateTimeOffset InvalidatedAt);

    private sealed class LinkRepository(List<string>? events = null) : IDeviceLinkRequestRepository
    {
        private readonly List<DeviceLinkRequest> _requests = [];
        public IReadOnlyList<DeviceLinkRequest> Requests => _requests;
        public List<DeviceLinkRequest> RequestsInternal => _requests;
        public List<InvalidationCall> InvalidationCalls { get; } = [];

        public Task AddAsync(DeviceLinkRequest request, CancellationToken ct = default)
        {
            _requests.Add(request);
            return Task.CompletedTask;
        }

        public Task<DeviceLinkRequest?> GetByDeviceCodeAsync(
            string deviceCode,
            CancellationToken ct = default) =>
            Task.FromResult(_requests.SingleOrDefault(request =>
                request.DeviceCode == deviceCode));

        public Task<DeviceLinkRequest?> GetByUserCodeAsync(
            string userCode,
            CancellationToken ct = default) =>
            Task.FromResult(_requests.SingleOrDefault(request =>
                request.UserCode == userCode));

        public Task<bool> IsUserCodeAvailableAsync(
            string userCode,
            CancellationToken ct = default) =>
            Task.FromResult(_requests.All(request => request.UserCode != userCode));

        public Task<IReadOnlyList<DeviceLinkRequest>> ListPendingForUserAsync(
            UserId userId,
            DateTimeOffset now,
            CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<DeviceLinkRequest>>(_requests
                .Where(request => request.UserId == userId && request.IsPending(now))
                .ToList());

        public void Update(DeviceLinkRequest request) { }

        public Task<DeviceLinkCancelOutcome> CancelPendingAsync(
            string userCode,
            UserId userId,
            DateTimeOffset cancelledAt,
            CancellationToken ct = default)
        {
            events?.Add("cancel");
            var request = _requests.SingleOrDefault(item =>
                item.UserCode == userCode && item.UserId == userId);
            if (request is null)
            {
                return Task.FromResult(DeviceLinkCancelOutcome.NotFound);
            }
            if (request.CancelledAt.HasValue)
            {
                return Task.FromResult(DeviceLinkCancelOutcome.AlreadyCancelled);
            }
            if (request.ApprovedAt.HasValue)
            {
                return Task.FromResult(DeviceLinkCancelOutcome.Approved);
            }
            if (!request.IsPending(cancelledAt))
            {
                return Task.FromResult(DeviceLinkCancelOutcome.Expired);
            }
            DomainFixtureHydrator.CancelDeviceLink(request, DateTimeOffset.UtcNow);
            return Task.FromResult(DeviceLinkCancelOutcome.Cancelled);
        }

        public Task<DeviceLinkAcknowledgeOutcome> AcknowledgeApprovedAsync(
            Guid requestId,
            UserId userId,
            string deviceId,
            DateTimeOffset now,
            CancellationToken ct = default) =>
            Task.FromResult(DeviceLinkAcknowledgeOutcome.Unavailable);

        public Task<int> InvalidateApprovedBeforeGenerationAsync(
            UserId userId,
            long committedGeneration,
            Guid? excludingRequestId,
            DateTimeOffset invalidatedAt,
            CancellationToken ct = default)
        {
            InvalidationCalls.Add(new InvalidationCall(
                userId,
                committedGeneration,
                excludingRequestId,
                invalidatedAt));
            var invalidated = 0;
            foreach (var request in _requests.Where(request =>
                         request.UserId == userId
                         && request.Id != excludingRequestId
                         && request.ApprovedAt.HasValue
                         && !request.AcknowledgedAt.HasValue
                         && !request.CancelledAt.HasValue
                         && request.DeviceListGeneration < committedGeneration))
            {
                DomainFixtureHydrator.CancelDeviceLink(request, invalidatedAt);
                invalidated++;
            }
            return Task.FromResult(invalidated);
        }

        public Task<int> InvalidateOutstandingForUserAsync(
            UserId userId,
            DateTimeOffset invalidatedAt,
            CancellationToken ct = default) => throw new NotSupportedException();

        public Task<int> DeleteStaleAsync(
            DateTimeOffset now,
            CancellationToken ct = default) => throw new NotSupportedException();
    }
}
