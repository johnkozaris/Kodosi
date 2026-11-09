using System.Text.Json;
using Kodosi.Accounts;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Missions;
using Kodosi.Plans;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class DeviceTests(PostgresFixture postgres)
{
    [Fact]
    public async Task DeviceRemovalPreservesOtherDevicesAndEndsOnlyTheTerminalsItHosted()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var second = new DeviceFixture(owner.User.Id, "second-device");
        var init = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof), TestContext.Current.CancellationToken), TestJson.Options);
        var nonce = Convert.ToBase64String(Enumerable.Repeat((byte)7, 16).ToArray());
        var retry = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.SigningKey), nonce, DeviceFixture.LinkProof), TestContext.Current.CancellationToken), TestJson.Options);
        Assert.Equal(init.GetProperty("requestId").GetGuid(), retry.GetProperty("requestId").GetGuid());
        var request = Assert.Single(JsonSerializer.SerializeToElement(
            await store.Devices.PendingLinksAsync(owner.User.Id, TestContext.Current.CancellationToken), TestJson.Options).EnumerateArray());
        Assert.Equal(nonce, request.GetProperty("nonce").GetString());
        Assert.Equal(DeviceFixture.LinkProof, request.GetProperty("proof").GetString());
        Assert.Equal(second.DeviceId, request.GetProperty("deviceId").GetString());
        Assert.Equal(Convert.ToBase64String(second.SigningKey), request.GetProperty("signingPublicKey").GetString());
        var requestId = init.GetProperty("requestId").GetGuid();
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var cert = second.CertificateBody(owner.Device.Id, now);
        var next = DeviceFixture.ListBody(owner.User.Id, 2, [(owner.Device.Id, owner.Device.Id), (second.DeviceId, owner.Device.Id)], owner.Device.Id, now);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Approved,
            new(requestId, Convert.ToBase64String(cert), owner.Fixture.SignedCertificate(cert), Convert.ToBase64String(next), owner.Fixture.SignedList(next), DeviceFixture.LinkProof), TestContext.Current.CancellationToken);
        Assert.Equal(second.DeviceId, (await store.Devices.RequireDeviceAsync(owner.User.Id, second.DeviceId, TestContext.Current.CancellationToken)).Id);
        Session Hosted(string device) => new() { Id = Guid.CreateVersion7(), IncarnationId = Guid.CreateVersion7(), OwnerUserId = owner.User.Id, HostDeviceId = device, HostName = "Host", Name = "Terminal" };
        var lost = Hosted(second.DeviceId); var kept = Hosted(owner.Device.Id);
        store.Db.Sessions.AddRange(lost, kept); await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        var removed = DeviceFixture.ListBody(owner.User.Id, 3, [(owner.Device.Id, owner.Device.Id)], owner.Device.Id, now + 1);
        await store.Devices.ReplaceListAsync(owner.User.Id, new(Convert.ToBase64String(removed), owner.Fixture.SignedList(removed)), TestContext.Current.CancellationToken);
        Assert.True(lost.Ended); Assert.False(kept.Ended); Assert.Equal(2, kept.AuthorizationRevision);
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.RequireDeviceAsync(owner.User.Id, second.DeviceId, TestContext.Current.CancellationToken));
        Assert.Equal(owner.Device.Id, (await store.Devices.RequireDeviceAsync(owner.User.Id, owner.Device.Id, TestContext.Current.CancellationToken)).Id);
        var readd = DeviceFixture.ListBody(owner.User.Id, 4, [(owner.Device.Id, owner.Device.Id), (second.DeviceId, owner.Device.Id)], owner.Device.Id, now + 2);
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.ReplaceListAsync(owner.User.Id, new(Convert.ToBase64String(readd), owner.Fixture.SignedList(readd)), TestContext.Current.CancellationToken));
    }

    private static DeviceService.RegisterDevice FirstDevice(DeviceService devices, DeviceFixture device, DateTimeOffset at)
    {
        var challenge = JsonSerializer.SerializeToElement(devices.CreateChallenge(device.UserId), TestJson.Options);
        var bytes = Convert.FromBase64String(challenge.GetProperty("challengeBytes").GetString()!);
        var now = at.ToUnixTimeMilliseconds();
        var cert = device.CertificateBody(device.DeviceId, now);
        var list = DeviceFixture.ListBody(device.UserId, 1, [(device.DeviceId, device.DeviceId)], device.DeviceId, now);
        return new(device.DeviceId, Convert.ToBase64String(device.SigningKey), challenge.GetProperty("challengeId").GetGuid(),
            Convert.ToBase64String(device.Sign(Proofs.Tagged(DomainTags.DevicePopV1, bytes))),
            Convert.ToBase64String(cert), device.SignedCertificate(cert), Convert.ToBase64String(list), device.SignedList(list));
    }

    private static DeviceService.ResetRequest Asking(DeviceFixture device) =>
        new(device.DeviceId, device.DeviceId, Convert.ToBase64String(device.SigningKey));

    [Fact]
    public async Task StartingFreshWaitsADayAndThenOnlyTheAskingDeviceReplacesTheIdentity()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var clock = new ManualClock();
        var devices = new DeviceService(store.Db, store.Connections, store.DeviceSessions, clock, store.Plans);
        var owner = await store.UserAsync("owner");
        var second = new DeviceFixture(owner.User.Id, "second-device");
        await devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof), ct);
        await store.Friends.ReplaceSignedListAsync(owner.User.Id, new(0, 1, "AQ==", Convert.ToBase64String(new byte[IdentityWireFormat.MlDsa65SignatureLength])), ct);
        var before = (await devices.IdentityAsync(owner.User.Id, owner.User.Id, null, ct)).Bundle!;
        var (fresh, other) = (new DeviceFixture(owner.User.Id, "fresh-device"), new DeviceFixture(owner.User.Id, "other-device"));
        var signedIn = clock.GetUtcNow().AddMinutes(-1);
        await Assert.ThrowsAsync<ApiException>(() => devices.RequestResetAsync(owner.User.Id, Asking(fresh), null, ct));
        await Assert.ThrowsAsync<ApiException>(() => devices.RequestResetAsync(owner.User.Id, Asking(fresh), signedIn - CurrentUser.ReauthenticationWindow, ct));
        var pending = JsonSerializer.SerializeToElement(await devices.RequestResetAsync(owner.User.Id, Asking(fresh), signedIn, ct), TestJson.Options);
        Assert.False(pending.GetProperty("ready").GetBoolean());
        var again = JsonSerializer.SerializeToElement(await devices.RequestResetAsync(owner.User.Id, Asking(fresh), signedIn, ct), TestJson.Options);
        Assert.Equal(pending.GetProperty("requestId").GetGuid(), again.GetProperty("requestId").GetGuid());
        await Assert.ThrowsAsync<ApiException>(() => devices.EnrollAsync(owner.User.Id, FirstDevice(devices, fresh, clock.GetUtcNow()), ct));
        clock.Advance(DeviceService.ResetWait);
        var ready = JsonSerializer.SerializeToElement(await devices.RequestResetAsync(owner.User.Id, Asking(fresh), signedIn, ct), TestJson.Options);
        Assert.True(ready.GetProperty("ready").GetBoolean());
        Assert.Equal(403, (await Assert.ThrowsAsync<ApiException>(() => devices.RequestResetAsync(owner.User.Id, Asking(other), signedIn, ct))).Status);
        await Assert.ThrowsAsync<ApiException>(() => devices.EnrollAsync(owner.User.Id, FirstDevice(devices, other, clock.GetUtcNow()), ct));
        Assert.NotNull(await devices.RequireDeviceAsync(owner.User.Id, owner.Device.Id, ct));
        await devices.EnrollAsync(owner.User.Id, FirstDevice(devices, fresh, clock.GetUtcNow()), ct);
        var after = (await devices.IdentityAsync(owner.User.Id, owner.User.Id, null, ct)).Bundle!;
        Assert.NotEqual(before.IdentityIncarnationId, after.IdentityIncarnationId);
        Assert.Single(after.Devices);
        await Assert.ThrowsAsync<ApiException>(() => devices.RequireDeviceAsync(owner.User.Id, owner.Device.Id, ct));
        Assert.Equal("cancelled", (await store.Db.DeviceLinks.AsNoTracking().SingleAsync(x => x.DeviceId == second.DeviceId, ct)).State);
        Assert.False(await store.Db.FriendLists.AnyAsync(x => x.UserId == owner.User.Id, ct));
        Assert.False(await store.Db.IdentityResets.AnyAsync(x => x.UserId == owner.User.Id, ct));
    }

    [Fact]
    public async Task ADeviceStopsStartingFreshAndTheFirstDeviceOfAnAccountRegistersAtOnce()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var fresh = new DeviceFixture(owner.User.Id, "fresh-device");
        var signedIn = DateTimeOffset.UtcNow.AddMinutes(-1);
        var pending = JsonSerializer.SerializeToElement(await store.Devices.RequestResetAsync(owner.User.Id, Asking(fresh), signedIn, ct), TestJson.Options);
        var requestId = pending.GetProperty("requestId").GetGuid();
        await store.Devices.CancelResetAsync(owner.User.Id, requestId, ct);
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.PendingResetAsync(owner.User.Id, ct));
        await store.Db.Devices.Where(x => x.Id == owner.Device.Id).ExecuteUpdateAsync(set => set.SetProperty(x => x.Revoked, true), ct);
        await store.Db.DeviceLists.Where(x => x.UserId == owner.User.Id).ExecuteDeleteAsync(ct);
        Assert.Equal(409, (await Assert.ThrowsAsync<ApiException>(() => store.Devices.RequestResetAsync(owner.User.Id, Asking(owner.Fixture), signedIn, ct))).Status);
        var newcomer = new User { Id = Guid.CreateVersion7(), Issuer = "https://test.example/", Subject = "newcomer", Handle = "newcomer", DisplayName = "newcomer" };
        store.Db.Users.Add(newcomer);
        await store.Db.SaveChangesAsync(ct);
        var first = new DeviceFixture(newcomer.Id, "first-device");
        Assert.Null(await store.Devices.RequestResetAsync(newcomer.Id, Asking(first), signedIn, ct));
        await store.Devices.EnrollAsync(newcomer.Id, FirstDevice(store.Devices, first, DateTimeOffset.UtcNow), ct);
        Assert.Equal(first.DeviceId, (await store.Devices.RequireDeviceAsync(newcomer.Id, first.DeviceId, ct)).Id);
    }

    [Fact]
    public async Task ExpiredOwnListCanOnlyBeUsedToRenewNotToControlSessions()
    {
        await using var store = await TestStore.CreateAsync(postgres); var owner = await store.UserAsync("owner");
        var stored = await store.Db.DeviceLists.SingleAsync(TestContext.Current.CancellationToken);
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var old = DeviceFixture.ListBody(owner.User.Id, 1, [(owner.Device.Id, owner.Device.Id)], owner.Device.Id, now - 120_000, now - 60_000);
        stored.Body = old; stored.Signature = owner.Fixture.Sign(Proofs.Tagged(DomainTags.DeviceListV1, old));
        stored.ExpiresAtMs = now - 60_000; stored.IssuedAtMs = now - 120_000; await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.RequireDeviceAsync(owner.User.Id, owner.Device.Id, TestContext.Current.CancellationToken));
        Assert.NotNull((await store.Devices.IdentityAsync(owner.User.Id, owner.User.Id, null, TestContext.Current.CancellationToken)).Bundle);
        var fresh = DeviceFixture.ListBody(owner.User.Id, 2, [(owner.Device.Id, owner.Device.Id)], owner.Device.Id, now, now + 3_600_000);
        await store.Devices.ReplaceListAsync(owner.User.Id, new(Convert.ToBase64String(fresh), owner.Fixture.SignedList(fresh)), TestContext.Current.CancellationToken);
        Assert.NotNull(await store.Devices.RequireDeviceAsync(owner.User.Id, owner.Device.Id, TestContext.Current.CancellationToken));
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ApprovalRejectsCertificateOutsideSignersValidityWithoutChangingIdentity(bool afterExpiry)
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        owner.Device.ExpiresAtMs = now + 60_000;
        owner.Device.Certificate = owner.Fixture.CertificateBody(owner.Device.Id, owner.Device.IssuedAtMs, owner.Device.ExpiresAtMs.Value);
        owner.Device.CertificateSignature = Convert.FromBase64String(owner.Fixture.SignedCertificate(owner.Device.Certificate));
        await store.Db.SaveChangesAsync(TestContext.Current.CancellationToken);
        var nextDevice = new DeviceFixture(owner.User.Id, "next-device");
        var init = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id,
            new(nextDevice.DeviceId, "Next", Convert.ToBase64String(nextDevice.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof), TestContext.Current.CancellationToken), TestJson.Options);
        var requestId = init.GetProperty("requestId").GetGuid();
        var issued = afterExpiry ? owner.Device.ExpiresAtMs.Value : owner.Device.IssuedAtMs - 1;
        var certificate = nextDevice.CertificateBody(owner.Device.Id, issued);
        var list = DeviceFixture.ListBody(owner.User.Id, 2,
            [(owner.Device.Id, owner.Device.Id), (nextDevice.DeviceId, owner.Device.Id)], owner.Device.Id, now);
        var error = await Assert.ThrowsAsync<ApiException>(() => store.Devices.ApproveLinkAsync(owner.User.Id, owner.Approved,
            new(requestId, Convert.ToBase64String(certificate), owner.Fixture.SignedCertificate(certificate), Convert.ToBase64String(list), owner.Fixture.SignedList(list), DeviceFixture.LinkProof), TestContext.Current.CancellationToken));
        Assert.Equal(400, error.Status);
        Assert.Contains("signer's validity", error.Message);
        Assert.Equal(1, (await store.Db.DeviceLists.SingleAsync(TestContext.Current.CancellationToken)).Generation);
        Assert.False(await store.Db.Devices.AnyAsync(x => x.Id == nextDevice.DeviceId, TestContext.Current.CancellationToken));
        Assert.Equal("pending", (await store.Db.DeviceLinks.SingleAsync(TestContext.Current.CancellationToken)).State);
        certificate = nextDevice.CertificateBody(owner.Device.Id, now);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Approved,
            new(requestId, Convert.ToBase64String(certificate), owner.Fixture.SignedCertificate(certificate), Convert.ToBase64String(list), owner.Fixture.SignedList(list), DeviceFixture.LinkProof), TestContext.Current.CancellationToken);
        Assert.NotNull(await store.Devices.RequireDeviceAsync(owner.User.Id, nextDevice.DeviceId, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task PredecessorAwareIssuanceAllowsRemovalWithinClockTolerance()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var second = new DeviceFixture(owner.User.Id, "second-device");
        var init = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof), TestContext.Current.CancellationToken), TestJson.Options);
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var future = now + 120_000;
        var certificate = second.CertificateBody(owner.Device.Id, future);
        var list = DeviceFixture.ListBody(owner.User.Id, 2,
            [(owner.Device.Id, owner.Device.Id), (second.DeviceId, owner.Device.Id)], owner.Device.Id, future);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Approved,
            new(init.GetProperty("requestId").GetGuid(), Convert.ToBase64String(certificate), owner.Fixture.SignedCertificate(certificate), Convert.ToBase64String(list), owner.Fixture.SignedList(list), DeviceFixture.LinkProof), TestContext.Current.CancellationToken);
        var removal = DeviceFixture.ListBody(owner.User.Id, 3, [(owner.Device.Id, owner.Device.Id)], owner.Device.Id, future + 1);
        await store.Devices.ReplaceListAsync(owner.User.Id,
            new(Convert.ToBase64String(removal), owner.Fixture.SignedList(removal)), TestContext.Current.CancellationToken);
        Assert.True((await store.Db.Devices.SingleAsync(x => x.Id == second.DeviceId, TestContext.Current.CancellationToken)).Revoked);
        Assert.Equal(3, (await store.Db.DeviceLists.SingleAsync(TestContext.Current.CancellationToken)).Generation);
    }

    [Fact]
    public void StrictIdentityParsersRejectTrailingBytesAndSignatureSubstitution()
    {
        var fixture = new DeviceFixture(Guid.CreateVersion7(), "device");
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(); var bytes = fixture.CertificateBody("device", now);
        Assert.Equal("device", DeviceCertificate.Parse(bytes).DeviceId);
        Assert.Throws<IdentityFormatException>(() => DeviceCertificate.Parse([.. bytes, 0]));
        var sig = fixture.Sign(Proofs.Tagged(DomainTags.DeviceCertV3, bytes));
        Assert.True(SignatureVerifier.Verify(fixture.SigningKey, Proofs.Tagged(DomainTags.DeviceCertV3, bytes), sig));
        bytes[^1] ^= 1; Assert.False(SignatureVerifier.Verify(fixture.SigningKey, Proofs.Tagged(DomainTags.DeviceCertV3, bytes), sig));
    }

    [Fact]
    public async Task ARecoveryKeyApprovesANewDeviceAndAnAccountHasOnlyOne()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var recovery = new DeviceFixture(owner.User.Id, "0a1b2c3d-4e5f-8a6b-9c7d-0e1f2a3b4c5d");
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var roomKey = new byte[MissionService.RoomPublicKeyLength];
        DeviceService.RecoveryKeyWrite Write(DeviceFixture entry, long generation, DeviceFixture roomKeySigner, params (string Device, string Signer)[] known)
        {
            var cert = entry.CertificateBody(owner.Device.Id, now);
            var list = DeviceFixture.ListBody(owner.User.Id, generation, [.. known, (entry.DeviceId, owner.Device.Id)], owner.Device.Id, now + generation);
            var signature = roomKeySigner.Sign(Proofs.RoomRecipient(owner.User.Id, entry.DeviceId, roomKey));
            return new(Convert.ToBase64String(cert), owner.Fixture.SignedCertificate(cert), Convert.ToBase64String(list), owner.Fixture.SignedList(list),
                Convert.ToBase64String(new byte[64]), new(Convert.ToBase64String(roomKey), Convert.ToBase64String(signature)));
        }
        var plain = new DeviceFixture(owner.User.Id, "a-computer");
        Assert.Equal(400, (await Assert.ThrowsAsync<ApiException>(() => store.Devices.AddRecoveryKeyAsync(owner.User.Id, owner.Approved,
            Write(plain, 2, plain, (owner.Device.Id, owner.Device.Id)), ct))).Status);
        Assert.Equal(400, (await Assert.ThrowsAsync<ApiException>(() => store.Devices.AddRecoveryKeyAsync(owner.User.Id, owner.Approved,
            Write(recovery, 2, owner.Fixture, (owner.Device.Id, owner.Device.Id)), ct))).Status);
        Assert.False(await store.Db.Devices.AnyAsync(x => x.Id == recovery.DeviceId, ct));
        await store.Devices.AddRecoveryKeyAsync(owner.User.Id, owner.Approved, Write(recovery, 2, recovery, (owner.Device.Id, owner.Device.Id)), ct);
        var box = JsonSerializer.SerializeToElement(await store.Devices.RecoveryBoxAsync(owner.User.Id, ct), TestJson.Options);
        Assert.Equal(recovery.DeviceId, box.GetProperty("deviceId").GetString());
        var keys = JsonSerializer.SerializeToElement(await store.Missions.RecipientKeysAsync(owner.User.Id, owner.User.Id, ct), TestJson.Options);
        Assert.Equal(recovery.DeviceId, Assert.Single(keys.EnumerateArray()).GetProperty("deviceId").GetString());
        var challenge = JsonSerializer.SerializeToElement(store.Devices.CreateChallenge(owner.User.Id), TestJson.Options).GetProperty("challengeId").GetGuid();
        var session = await Assert.ThrowsAsync<ApiException>(() => store.Devices.OpenSessionAsync(owner.User.Id,
            new(recovery.DeviceId, challenge, Convert.ToBase64String(new byte[IdentityWireFormat.MlDsa65SignatureLength])), ct));
        Assert.Equal(403, session.Status);
        Assert.Contains("recovery key", session.Message);
        var replacement = new DeviceFixture(owner.User.Id, "1a1b2c3d-4e5f-8a6b-9c7d-0e1f2a3b4c5d");
        var second = await Assert.ThrowsAsync<ApiException>(() => store.Devices.AddRecoveryKeyAsync(owner.User.Id, owner.Approved,
            Write(replacement, 3, replacement, (owner.Device.Id, owner.Device.Id), (recovery.DeviceId, owner.Device.Id)), ct));
        Assert.Equal(409, second.Status);

        var next = new DeviceFixture(owner.User.Id, "new-computer");
        var cert = next.CertificateBody(recovery.DeviceId, now + 10);
        var list = DeviceFixture.ListBody(owner.User.Id, 3,
            [(owner.Device.Id, owner.Device.Id), (recovery.DeviceId, owner.Device.Id), (next.DeviceId, recovery.DeviceId)], recovery.DeviceId, now + 10);
        DeviceService.RegisterDevice Request(DeviceFixture signer)
        {
            var challenge = JsonSerializer.SerializeToElement(store.Devices.CreateChallenge(owner.User.Id), TestJson.Options);
            var proof = next.Sign(Proofs.Tagged(DomainTags.DevicePopV1, Convert.FromBase64String(challenge.GetProperty("challengeBytes").GetString()!)));
            return new(next.DeviceId, Convert.ToBase64String(next.SigningKey), challenge.GetProperty("challengeId").GetGuid(), Convert.ToBase64String(proof),
                Convert.ToBase64String(cert), signer.SignedCertificate(cert), Convert.ToBase64String(list), signer.SignedList(list));
        }
        var forged = await Assert.ThrowsAsync<ApiException>(() => store.Devices.RecoverAsync(owner.User.Id, Request(owner.Fixture), ct));
        Assert.Equal(403, forged.Status);
        Assert.False(await store.Db.Devices.AnyAsync(x => x.Id == next.DeviceId, ct));
        await store.Devices.RecoverAsync(owner.User.Id, Request(recovery), ct);
        Assert.NotNull(await store.Devices.RequireDeviceAsync(owner.User.Id, next.DeviceId, ct));
        Assert.Equal(3, (await store.Db.DeviceLists.SingleAsync(x => x.UserId == owner.User.Id, ct)).Generation);

        var other = await store.UserAsync("other");
        Assert.Equal(404, (await Assert.ThrowsAsync<ApiException>(() => store.Devices.RecoveryBoxAsync(other.User.Id, ct))).Status);
    }

    [Fact]
    public async Task TheDeviceLimitStopsARecoveryAndOffersAFreshStart()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        store.Plans = PlanCatalog.Of("one", new Dictionary<string, PlanLimits> { ["one"] = PlanLimits.Safety with { Devices = 1 } });
        var owner = await store.UserAsync("owner");
        var recovery = new DeviceFixture(owner.User.Id, "0a1b2c3d-4e5f-8a6b-9c7d-0e1f2a3b4c5d");
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var roomKey = new byte[MissionService.RoomPublicKeyLength];
        var keyCert = recovery.CertificateBody(owner.Device.Id, now);
        var keyList = DeviceFixture.ListBody(owner.User.Id, 2, [(owner.Device.Id, owner.Device.Id), (recovery.DeviceId, owner.Device.Id)], owner.Device.Id, now);
        await store.Devices.AddRecoveryKeyAsync(owner.User.Id, owner.Approved, new(Convert.ToBase64String(keyCert), owner.Fixture.SignedCertificate(keyCert),
            Convert.ToBase64String(keyList), owner.Fixture.SignedList(keyList), Convert.ToBase64String(new byte[64]),
            new(Convert.ToBase64String(roomKey), Convert.ToBase64String(recovery.Sign(Proofs.RoomRecipient(owner.User.Id, recovery.DeviceId, roomKey))))), ct);

        var next = new DeviceFixture(owner.User.Id, "new-computer");
        var cert = next.CertificateBody(recovery.DeviceId, now + 10);
        var list = DeviceFixture.ListBody(owner.User.Id, 3,
            [(owner.Device.Id, owner.Device.Id), (recovery.DeviceId, owner.Device.Id), (next.DeviceId, recovery.DeviceId)], recovery.DeviceId, now + 10);
        var challenge = JsonSerializer.SerializeToElement(store.Devices.CreateChallenge(owner.User.Id), TestJson.Options);
        var proof = next.Sign(Proofs.Tagged(DomainTags.DevicePopV1, Convert.FromBase64String(challenge.GetProperty("challengeBytes").GetString()!)));
        var refusal = await Assert.ThrowsAsync<ApiException>(() => store.Devices.RecoverAsync(owner.User.Id, new(next.DeviceId,
            Convert.ToBase64String(next.SigningKey), challenge.GetProperty("challengeId").GetGuid(), Convert.ToBase64String(proof),
            Convert.ToBase64String(cert), recovery.SignedCertificate(cert), Convert.ToBase64String(list), recovery.SignedList(list)), ct));
        Assert.Equal(403, refusal.Status);
        Assert.Equal("This account has 1 device, the most that it can have. Remove a device in Settings on one of your devices, then try again. "
            + "If you have no other device, start fresh on this device.", refusal.Message);
        Assert.False(await store.Db.Devices.AnyAsync(x => x.Id == next.DeviceId, ct));
        Assert.Equal(2, (await store.Db.DeviceLists.SingleAsync(ct)).Generation);
    }
}
