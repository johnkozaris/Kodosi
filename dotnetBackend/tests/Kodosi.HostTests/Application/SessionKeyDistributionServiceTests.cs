using Kodosi.Application;
using Kodosi.Domain;

using static Kodosi.HostTests.SecurityAuthorizationTestFactory;

namespace Kodosi.HostTests;

public sealed class SessionKeyDistributionServiceTests
{
    [Fact]
    public async Task ReplaceKeyBlobsAsync_ReplacesExistingBlobs_WhenPayloadIsValid()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var existingBlob = CreateBlob(session.Id, "old-device", senderDevice);
        var keyBlobs = new TrackingSessionKeyBlobRepository(existingBlob);
        var unitOfWork = new TrackingUnitOfWork();
        var sessions = new FakeSessionRepository(session);
        var signatureVerifier = new SequenceSignatureVerifier(true);
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            sessions,
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            signatureVerifier,
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.Stored, result.State);
        Assert.Equal(1, keyBlobs.DeleteCalls);
        Assert.Equal(1, keyBlobs.AddRangeCalls);
        Assert.Equal(1, unitOfWork.SaveCalls);
        Assert.Equal(1, sessions.GetByIdForUpdateCalls);
        Assert.Equal(1, unitOfWork.BeginCalls);
        Assert.Equal(1, signatureVerifier.VerifyCalls);
        Assert.Null(await keyBlobs.GetForDeviceAsync(
            session.Id,
            existingBlob.RecipientDeviceId,
            TestContext.Current.CancellationToken));

        var storedBlob = await keyBlobs.GetForDeviceAsync(
            session.Id,
            "recipient-1",
            TestContext.Current.CancellationToken);
        Assert.NotNull(storedBlob);
        Assert.Equal(senderDevice.DeviceId, storedBlob!.SenderDeviceId);
        Assert.Equal(0, storedBlob.KeyGeneration);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_Accepts_Explicit_Legacy_Digest_For_Legacy_Incarnation()
    {
        var ownerId = UserId.New();
        var session = Session.Create(
            SessionId.New(),
            Guid.CreateVersion7(),
            1,
            Session.LegacyIncarnationProtocolVersion,
            ownerId,
            "Session",
            Domain.SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.Suggest,
            "owner-secret-hash");
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var authorization = CreateAuthorization(senderDevice, "recipient-1");
        var signatureVerifier = new SequenceSignatureVerifier(true);
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            signatureVerifier,
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            new TrackingUnitOfWork(),
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    session.CurrentKeyGeneration,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.LegacyVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.Stored, result.State);
        Assert.Equal(1, signatureVerifier.VerifyCalls);
        var storedBlob = await keyBlobs.GetForDeviceAsync(
            session.Id,
            "recipient-1",
            TestContext.Current.CancellationToken);
        Assert.NotNull(storedBlob);
        Assert.Equal(SessionKeyBlobSignatureDigest.LegacyVersion, storedBlob.SignatureVersion);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_Rejects_Legacy_Digest_For_ServerIssued_Incarnation()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var authorization = CreateAuthorization(senderDevice, "recipient-1");
        var signatureVerifier = new SequenceSignatureVerifier(true);
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            new TrackingSessionKeyBlobRepository(),
            authorization.Devices,
            authorization.DeviceLists,
            signatureVerifier,
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            new TrackingUnitOfWork(),
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    session.CurrentKeyGeneration,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.LegacyVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Contains("legacy session incarnation", result.ErrorMessage);
        Assert.Equal(0, signatureVerifier.VerifyCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_AcquiresSessionAuthorityBeforeSessionRowLock()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var lockOrder = new List<string>();
        var service = new SessionKeyDistributionService(
            new LockOrderSessionRepository(session, lockOrder),
            new TrackingSessionKeyBlobRepository(),
            new FakeUserDeviceRepository(),
            new FakeUserDeviceListRepository(),
            new AlwaysValidSignatureVerifier(),
            new LockOrderLifecycleLock(lockOrder),
            new FakeRecipientDeviceLifecycleLock(),
            new LockOrderSessionEndAuthority(lockOrder),
            CreateSessionAccessService(),
            new TrackingUnitOfWork(),
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal(["user", "authority", "session"], lockOrder);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsRecipientWhoseSessionAccessWasRevoked()
    {
        var ownerId = UserId.New();
        var recipientUserId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var recipientDevice = CreateDevice(recipientUserId, "recipient-1");
        var accessOverride = SessionAccessOverride.Create(
            session.Id,
            recipientUserId,
            AccessLevel.View,
            ownerId,
            DateTimeOffset.UtcNow.AddHours(1),
            DateTimeOffset.UtcNow);
        accessOverride.Revoke();
        var keyBlobs = new TrackingSessionKeyBlobRepository(
            CreateBlob(session.Id, "old-device", senderDevice));
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            new FakeUserDeviceRepository(senderDevice, recipientDevice),
            new FakeUserDeviceListRepository(
                CreateDeviceList(senderDevice),
                CreateDeviceList(recipientDevice)),
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            CreateSessionAccessService(
                overrides: new FakeAccessOverrideRepository(accessOverride)),
            new TrackingUnitOfWork(),
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    recipientDevice.DeviceId,
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    session.CurrentKeyGeneration,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal(
            $"Recipient device {recipientDevice.DeviceId} is not authorized for the session.",
            result.ErrorMessage);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsTamperedSignatureWithoutMutatingStoredBlobs()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var existingBlob = CreateBlob(session.Id, "old-device", senderDevice);
        var keyBlobs = new TrackingSessionKeyBlobRepository(existingBlob);
        var unitOfWork = new TrackingUnitOfWork();
        var signatureVerifier = new SequenceSignatureVerifier(false);
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            signatureVerifier,
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal("Invalid key blob signature for device recipient-1.", result.ErrorMessage);
        Assert.Equal(1, signatureVerifier.VerifyCalls);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
        Assert.NotNull(await keyBlobs.GetForDeviceAsync(
            session.Id,
            existingBlob.RecipientDeviceId,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_LateSignatureFailureLeavesStoredBlobsUnchanged()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var existingBlob = CreateBlob(session.Id, "old-device", senderDevice);
        var keyBlobs = new TrackingSessionKeyBlobRepository(existingBlob);
        var unitOfWork = new TrackingUnitOfWork();
        var signatureVerifier = new SequenceSignatureVerifier(true, false);
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1",
            "recipient-2");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            signatureVerifier,
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);
        var issuedAtMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    issuedAtMs,
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion),
                new SessionKeyBlobSubmission(
                    "recipient-2",
                    Convert.ToBase64String([7, 8, 9]),
                    senderDevice.DeviceId,
                    0,
                    issuedAtMs,
                    Convert.ToBase64String([10, 11, 12]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal("Invalid key blob signature for device recipient-2.", result.ErrorMessage);
        Assert.Equal(2, signatureVerifier.VerifyCalls);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
        Assert.NotNull(await keyBlobs.GetForDeviceAsync(
            session.Id,
            existingBlob.RecipientDeviceId,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsExpiredSenderCertificate()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var certificateExpiry = DateTimeOffset.UtcNow.AddMinutes(1);
        var senderDevice = CreateDevice(
            ownerId,
            $"{ownerId.Value}:desktop",
            certificateExpiry);
        var clock = new TestTimeProvider(certificateExpiry.AddMilliseconds(1));
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            clock);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    clock.GetUtcNow().ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal($"Sender device {senderDevice.DeviceId} is not active.", result.ErrorMessage);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsUnlistedSenderDevice()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var senderList = CreateDeviceList(
            senderDevice,
            expiresAt: null,
            $"{ownerId.Value}:other");
        var authorization = CreateAuthorization(
            senderDevice,
            senderList,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal($"Sender device {senderDevice.DeviceId} is not active.", result.ErrorMessage);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsRevokedSenderDevice()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        senderDevice.Revoke(senderDevice.DeviceId);
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal($"Sender device {senderDevice.DeviceId} is not active.", result.ErrorMessage);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsDuplicateRecipientDeviceIdsAtomically()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var existingBlob = CreateBlob(session.Id, "old-device", senderDevice);
        var keyBlobs = new TrackingSessionKeyBlobRepository(existingBlob);
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);
        var issuedAtMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    issuedAtMs,
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion),
                new SessionKeyBlobSubmission(
                    " recipient-1 ",
                    Convert.ToBase64String([7, 8, 9]),
                    senderDevice.DeviceId,
                    0,
                    issuedAtMs,
                    Convert.ToBase64String([10, 11, 12]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal("Duplicate recipient device ID recipient-1.", result.ErrorMessage);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
        Assert.NotNull(await keyBlobs.GetForDeviceAsync(
            session.Id,
            existingBlob.RecipientDeviceId,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsInvalidEncryptedKeyWithoutMutatingStoredBlobs()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var existingBlob = CreateBlob(session.Id, "old-device", senderDevice);
        var keyBlobs = new TrackingSessionKeyBlobRepository(existingBlob);
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    "not-base64",
                    senderDevice.DeviceId,
                    7,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal("Invalid base64 in encrypted key for device recipient-1.", result.ErrorMessage);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
        Assert.NotNull(await keyBlobs.GetForDeviceAsync(
            session.Id,
            existingBlob.RecipientDeviceId,
            TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsNullEncryptedKey_AsBadRequest()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            new TrackingSessionKeyBlobRepository(),
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            new TrackingUnitOfWork(),
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    null!,
                    senderDevice.DeviceId,
                    0,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal("Invalid base64 in encrypted key for device recipient-1.", result.ErrorMessage);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsNullSenderDeviceId_AsBadRequest()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var authorization = CreateAuthorization(
            senderDevice: null,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            new TrackingSessionKeyBlobRepository(),
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            new TrackingUnitOfWork(),
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    null!,
                    0,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal("SenderDeviceId is required for device recipient-1.", result.ErrorMessage);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsBlankRecipientDeviceId_AsBadRequest()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            new FakeUserDeviceRepository(senderDevice),
            new FakeUserDeviceListRepository(CreateDeviceList(senderDevice)),
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            CreateSessionAccessService(),
            new TrackingUnitOfWork(),
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    " ",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal("Recipient device ID is required.", result.ErrorMessage);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsKeyGenerationMismatch_AsBadRequest()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    1,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Contains("does not match the session's current generation", result.ErrorMessage);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsPastKeyGeneration_AsBadRequest()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        SetCurrentKeyGeneration(session, 5);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    2,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Contains("does not match the session's current generation", result.ErrorMessage);
        Assert.Contains("current generation 5", result.ErrorMessage);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_AcceptsMatchingAdvancedKeyGeneration()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        SetCurrentKeyGeneration(session, 7);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    7,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.Stored, result.State);
        Assert.Equal(1, keyBlobs.AddRangeCalls);
    }

    private static void SetCurrentKeyGeneration(Session session, int value)
    {
        typeof(Session)
            .GetProperty(nameof(Session.CurrentKeyGeneration))!
            .SetValue(session, value);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsNegativeKeyGeneration_AsBadRequest()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    -1,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Equal("KeyGeneration must be non-negative.", result.ErrorMessage);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Theory]
    [InlineData(0L, "positive")]
    [InlineData(-1L, "positive")]
    [InlineData(long.MinValue, "positive")]
    public async Task ReplaceKeyBlobsAsync_RejectsNonPositiveIssuedAtMs_AsBadRequest(
        long issuedAtMs,
        string expectedMessageFragment)
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    issuedAtMs,
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Contains(expectedMessageFragment, result.ErrorMessage!, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_RejectsLongMaxValueIssuedAtMs_WithoutOverflow()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    long.MaxValue,
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Contains("drifted", result.ErrorMessage!, StringComparison.OrdinalIgnoreCase);
    }

    [Theory]
    [InlineData(-(15 * 60 * 1000 + 1))]
    [InlineData(15 * 60 * 1000 + 1)]
    public async Task ReplaceKeyBlobsAsync_RejectsIssuedAtMsOutsideSkewWindow_AsBadRequest(
        long offsetFromNowMs)
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var clock = new TestTimeProvider(new DateTimeOffset(2026, 4, 26, 12, 0, 0, TimeSpan.Zero));
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            clock);

        var issuedAtMs = clock.GetUtcNow().ToUnixTimeMilliseconds() + offsetFromNowMs;

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    issuedAtMs,
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.InvalidRequest, result.State);
        Assert.Contains("drifted", result.ErrorMessage!, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_ReturnsNotFound_WhenActorDoesNotOwnSession()
    {
        var ownerId = UserId.New();
        var outsiderId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            outsiderId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.NotFound, result.State);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_Rejects_Stale_Incarnation_Before_Mutation()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.JustMe);
        var staleIncarnationId = session.IncarnationId;
        session.End();
        session.Republish(Guid.CreateVersion7(), checked(session.IncarnationGeneration + 1),
            ownerId,
            "republished",
            Domain.SessionScope.JustMe,
            ToolKind.Terminal,
            AccessLevel.View,
            "new-secret",
            roomId: null);
        Assert.NotEqual(staleIncarnationId, session.IncarnationId);
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            new FakeUserDeviceRepository(),
            new FakeUserDeviceListRepository(),
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            CreateSessionAccessService(),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            staleIncarnationId,
            [],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.StaleIncarnation, result.State);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    [Fact]
    public async Task ReplaceKeyBlobsAsync_ReturnsNotFound_WhenSessionIsEnded()
    {
        var ownerId = UserId.New();
        var session = CreateSession(ownerId, Domain.SessionScope.Friends);
        session.ActivateHost("test-host");
        session.ReleaseHostSlot("test-host");
        session.End();
        var senderDevice = CreateDevice(ownerId, $"{ownerId.Value}:desktop");
        var keyBlobs = new TrackingSessionKeyBlobRepository();
        var unitOfWork = new TrackingUnitOfWork();
        var authorization = CreateAuthorization(
            senderDevice,
            "recipient-1");
        var service = new SessionKeyDistributionService(
            new FakeSessionRepository(session),
            keyBlobs,
            authorization.Devices,
            authorization.DeviceLists,
            new AlwaysValidSignatureVerifier(),
            new FakeUserLifecycleLock(),
            new FakeRecipientDeviceLifecycleLock(),
            new FakeSessionEndAuthority(),
            authorization.ForSession(session),
            unitOfWork,
            new NoOpSharingMetrics(),
            TimeProvider.System);

        var result = await service.ReplaceKeyBlobsAsync(
            session.Id,
            ownerId,
            session.IncarnationId,
            [
                new SessionKeyBlobSubmission(
                    "recipient-1",
                    Convert.ToBase64String([1, 2, 3]),
                    senderDevice.DeviceId,
                    0,
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Convert.ToBase64String([4, 5, 6]),
                    SessionKeyBlobSignatureDigest.CurrentVersion)
            ],
            TestContext.Current.CancellationToken);

        Assert.Equal(StoreSessionKeyBlobsState.NotFound, result.State);
        Assert.Equal(0, keyBlobs.DeleteCalls);
        Assert.Equal(0, keyBlobs.AddRangeCalls);
        Assert.Equal(0, unitOfWork.SaveCalls);
    }

    private static SessionAccessService CreateSessionAccessService(
        IFriendshipRepository? friendships = null,
        IRoomMemberRepository? roomMembers = null,
        IAccessOverrideRepository? overrides = null,
        ISessionViewerDismissalRepository? dismissals = null,
        TimeProvider? timeProvider = null) =>
        new(
            friendships ?? new FakeFriendshipRepository(),
            roomMembers ?? new FakeRoomMemberRepository(),
            overrides ?? new FakeAccessOverrideRepository(),
            dismissals ?? new FakeSessionViewerDismissalRepository(),
            timeProvider);

    private static UserDevice CreateDevice(
        UserId userId,
        string deviceId,
        DateTimeOffset? certificateExpiry = null)
    {
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            deviceId,
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Test device",
            signerDeviceId: deviceId,
            issuedAt: DateTimeOffset.UtcNow.AddMinutes(-1),
            expiresAt: certificateExpiry);
        return device;
    }

    private static UserDeviceList CreateDeviceList(
        UserDevice device,
        DateTimeOffset? expiresAt = null,
        params string[] deviceIds)
    {
        var listedDeviceIds = deviceIds.Length > 0
            ? deviceIds
            : [device.DeviceId];
        return TestDeviceList.Create(
            device.UserId,
            1,
            TestDeviceList.Entries(
                listedDeviceIds.Select(deviceId => (deviceId, deviceId))),
            listedDeviceIds[0],
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-1).ToUnixTimeMilliseconds(),
            expiresAt?.ToUnixTimeMilliseconds());
    }

    private static DeviceAuthorizationFixture CreateAuthorization(
        UserDevice? senderDevice,
        params string[] recipientDeviceIds) =>
        CreateAuthorization(
            senderDevice,
            senderDevice is null ? null : CreateDeviceList(senderDevice),
            recipientDeviceIds);

    private static DeviceAuthorizationFixture CreateAuthorization(
        UserDevice? senderDevice,
        UserDeviceList? senderDeviceList,
        params string[] recipientDeviceIds)
    {
        var recipientUserId = UserId.New();
        var recipientDevices = recipientDeviceIds
            .Select(deviceId => CreateDevice(recipientUserId, deviceId))
            .ToArray();
        var devices = senderDevice is null
            ? recipientDevices
            : [senderDevice, .. recipientDevices];
        var lists = new List<UserDeviceList>();
        if (senderDeviceList is not null)
        {
            lists.Add(senderDeviceList);
        }
        if (recipientDevices.Length > 0)
        {
            lists.Add(CreateDeviceList(
                recipientDevices[0],
                expiresAt: null,
                recipientDeviceIds));
        }

        return new DeviceAuthorizationFixture(
            new FakeUserDeviceRepository(devices),
            new FakeUserDeviceListRepository([.. lists]),
            recipientDevices.Select(device => device.UserId).Distinct().ToArray(),
            senderDevice?.UserId ?? UserId.New());
    }

    private sealed record DeviceAuthorizationFixture(
        FakeUserDeviceRepository Devices,
        FakeUserDeviceListRepository DeviceLists,
        IReadOnlyList<UserId> RecipientUserIds,
        UserId OwnerId)
    {
        public SessionAccessService ForSession(Session session) =>
            CreateSessionAccessService(
                overrides: new FakeAccessOverrideRepository(
                    RecipientUserIds.Select(recipientUserId =>
                        SessionAccessOverride.Create(
                            session.Id,
                            recipientUserId,
                            AccessLevel.View,
                            OwnerId,
                            DateTimeOffset.UtcNow.AddHours(1),
                            DateTimeOffset.UtcNow))
                        .ToArray()));
    }

    private sealed class SequenceSignatureVerifier(params bool[] results) : IPopSignatureVerifier
    {
        private readonly Queue<bool> _results = new(results);

        public int VerifyCalls { get; private set; }

        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature)
        {
            VerifyCalls++;
            return _results.Dequeue();
        }
    }

    private sealed class LockOrderLifecycleLock(List<string> lockOrder) : IUserLifecycleLock
    {
        public Task AcquireAsync(UserId userId, CancellationToken ct = default)
        {
            lockOrder.Add("user");
            return Task.CompletedTask;
        }
    }

    private sealed class LockOrderSessionEndAuthority(List<string> lockOrder)
        : ISessionEndAuthority
    {
        public ValueTask<IAsyncDisposable> AcquireAsync(
            IReadOnlyCollection<SessionId> sessionIds,
            CancellationToken ct = default)
        {
            lockOrder.Add("authority");
            return ValueTask.FromResult<IAsyncDisposable>(new EmptyLease());
        }

        public Task ProjectCommittedAsync(
            IReadOnlyCollection<CommittedSessionEnd> sessionEnds,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        public Task RetireEndedIncarnationAsync(
            SessionDiscoveryTarget endedSession,
            DateTimeOffset startedAt,
            CancellationToken ct = default) =>
            Task.CompletedTask;

        private sealed class EmptyLease : IAsyncDisposable
        {
            public ValueTask DisposeAsync() => ValueTask.CompletedTask;
        }
    }

    private sealed class LockOrderSessionRepository(
        Session session,
        List<string> lockOrder) : SessionRepositoryStub
    {
        public override Task<Session?> GetByIdForUpdateAsync(
            SessionId id,
            CancellationToken ct = default)
        {
            lockOrder.Add("session");
            return Task.FromResult<Session?>(id == session.Id ? session : null);
        }
    }

    private sealed class NoOpSharingMetrics : ISharingMetrics
    {
        public void RecordSessionKeySkewRejection() { }
    }

    private static SessionKeyBlob CreateBlob(SessionId sessionId, string recipientDeviceId, UserDevice senderDevice) =>
        SessionKeyBlob.Create(
            sessionId,
            recipientDeviceId,
            [1, 2, 3],
            senderDevice.DeviceId,
            senderDevice.KemPublicKey,
            1,
            SessionKeyBlobSignatureDigest.LegacyVersion,
            [7, 8, 9],
            issuedAtMs: 1_000L);

    private sealed class TrackingSessionKeyBlobRepository(params SessionKeyBlob[] blobs) : ISessionKeyBlobRepository
    {
        private readonly Dictionary<(SessionId SessionId, string RecipientDeviceId), SessionKeyBlob> _blobs = blobs
            .ToDictionary(
                blob => (blob.SessionId, blob.RecipientDeviceId),
                blob => blob);

        public int DeleteCalls { get; private set; }
        public int AddRangeCalls { get; private set; }

        public Task<SessionKeyBlob?> GetForDeviceAsync(
            SessionId sessionId,
            string recipientDeviceId,
            CancellationToken ct = default)
        {
            _blobs.TryGetValue((sessionId, recipientDeviceId), out var blob);
            return Task.FromResult(blob);
        }

        public Task AddRangeAsync(IReadOnlyList<SessionKeyBlob> blobs, CancellationToken ct = default)
        {
            AddRangeCalls++;
            foreach (var blob in blobs)
            {
                _blobs[(blob.SessionId, blob.RecipientDeviceId)] = blob;
            }

            return Task.CompletedTask;
        }

        public Task DeleteForSessionAsync(SessionId sessionId, CancellationToken ct = default)
        {
            DeleteCalls++;
            foreach (var key in _blobs.Keys.Where(key => key.SessionId == sessionId).ToArray())
            {
                _blobs.Remove(key);
            }

            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<DeviceRevocationSessionTarget>>
            GetSessionTargetsForRecipientDevicesAsync(
                IReadOnlyCollection<string> recipientDeviceIds,
                CancellationToken ct = default) =>
            throw new NotSupportedException();

        public Task<IReadOnlyList<SessionId>> GetSessionIdsForRecipientDevicesAsync(
            IReadOnlyCollection<string> recipientDeviceIds,
            CancellationToken ct = default)
        {
            IReadOnlyList<SessionId> sessionIds = _blobs.Keys
                .Where(key => recipientDeviceIds.Contains(key.RecipientDeviceId))
                .Select(key => key.SessionId)
                .Distinct()
                .ToList();
            return Task.FromResult(sessionIds);
        }

        public Task<int> DeleteForRecipientDevicesAsync(
            IReadOnlyCollection<string> recipientDeviceIds,
            CancellationToken ct = default)
        {
            var removed = 0;
            foreach (var key in _blobs.Keys
                .Where(key => recipientDeviceIds.Contains(key.RecipientDeviceId))
                .ToArray())
            {
                _blobs.Remove(key);
                removed++;
            }

            return Task.FromResult(removed);
        }
    }

    private sealed class TrackingUnitOfWork : UnitOfWorkStub
    {
        public int SaveCalls { get; private set; }
        public int BeginCalls { get; private set; }

        public override Task SaveChangesAsync(CancellationToken ct = default)
        {
            SaveCalls++;
            return Task.CompletedTask;
        }

        public override Task<ITransactionScope> BeginTransactionAsync(CancellationToken ct = default)
        {
            BeginCalls++;
            return Task.FromResult<ITransactionScope>(new TrackingTransaction());
        }

        private sealed class TrackingTransaction : TransactionScopeStub
        {
            public override Task CommitAsync(CancellationToken ct = default) => Task.CompletedTask;
            public override ValueTask DisposeAsync() => ValueTask.CompletedTask;
        }
    }
}
