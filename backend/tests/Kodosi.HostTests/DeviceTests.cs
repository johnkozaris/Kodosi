using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.TerminalConnections;
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
            new(second.DeviceId, "Second", Convert.ToBase64String(second.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof), TestContext.Current.CancellationToken), Wire.Json);
        var nonce = Convert.ToBase64String(Enumerable.Repeat((byte)7, 16).ToArray());
        var retry = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.SigningKey), nonce, DeviceFixture.LinkProof), TestContext.Current.CancellationToken), Wire.Json);
        Assert.Equal(init.GetProperty("requestId").GetGuid(), retry.GetProperty("requestId").GetGuid());
        var request = Assert.Single(JsonSerializer.SerializeToElement(
            await store.Devices.PendingLinksAsync(owner.User.Id, TestContext.Current.CancellationToken), Wire.Json).EnumerateArray());
        Assert.Equal(nonce, request.GetProperty("nonce").GetString());
        Assert.Equal(DeviceFixture.LinkProof, request.GetProperty("proof").GetString());
        Assert.Equal(second.DeviceId, request.GetProperty("deviceId").GetString());
        Assert.Equal(Convert.ToBase64String(second.SigningKey), request.GetProperty("signingPublicKey").GetString());
        Assert.False(request.TryGetProperty("kemPublicKey", out _));
        var requestId = init.GetProperty("requestId").GetGuid();
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var cert = second.CertificateBody(owner.Device.Id, now);
        var next = DeviceFixture.ListBody(owner.User.Id, 2, [(owner.Device.Id, owner.Device.Id), (second.DeviceId, owner.Device.Id)], owner.Device.Id, now);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Device,
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

    [Fact]
    public async Task ResetRevokesEveryDeviceAndAllowsAFreshFirstDevice()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var second = new DeviceFixture(owner.User.Id, "second-device");
        await store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof), ct);
        var before = (await store.Devices.IdentityAsync(owner.User.Id, owner.User.Id, null, ct)).Bundle!;
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.ResetIdentityAsync(owner.User.Id, null, ct));
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.ResetIdentityAsync(owner.User.Id, DateTimeOffset.UtcNow - DeviceService.ReauthenticationWindow - TimeSpan.FromSeconds(1), ct));
        Assert.NotNull(await store.Devices.RequireDeviceAsync(owner.User.Id, owner.Device.Id, ct));
        await store.Devices.ResetIdentityAsync(owner.User.Id, DateTimeOffset.UtcNow.AddMinutes(-1), ct);
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.IdentityAsync(owner.User.Id, owner.User.Id, null, ct));
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.RequireDeviceAsync(owner.User.Id, owner.Device.Id, ct));
        Assert.True((await store.Db.Devices.AsNoTracking().SingleAsync(x => x.Id == owner.Device.Id, ct)).Revoked);
        Assert.Equal("cancelled", (await store.Db.DeviceLinks.AsNoTracking().SingleAsync(x => x.DeviceId == second.DeviceId, ct)).State);
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof), ct));
        var fresh = new DeviceFixture(owner.User.Id, "fresh-device");
        var challenge = JsonSerializer.SerializeToElement(store.Devices.CreateChallenge(owner.User.Id), Wire.Json);
        var challengeBytes = Convert.FromBase64String(challenge.GetProperty("challengeBytes").GetString()!);
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var cert = fresh.CertificateBody(fresh.DeviceId, now);
        var list = DeviceFixture.ListBody(owner.User.Id, 1, [(fresh.DeviceId, fresh.DeviceId)], fresh.DeviceId, now);
        await store.Devices.EnrollAsync(owner.User.Id, new(fresh.DeviceId, Convert.ToBase64String(fresh.SigningKey),
            challenge.GetProperty("challengeId").GetGuid(), Convert.ToBase64String(fresh.Sign(Proofs.Tagged(DomainTags.DevicePopV1, challengeBytes))),
            Convert.ToBase64String(cert), fresh.SignedCertificate(cert), Convert.ToBase64String(list), fresh.SignedList(list)), ct);
        var after = (await store.Devices.IdentityAsync(owner.User.Id, owner.User.Id, null, ct)).Bundle!;
        Assert.NotEqual(before.IdentityIncarnationId, after.IdentityIncarnationId);
        Assert.Single(after.Devices);
        Assert.Equal(fresh.DeviceId, (await store.Devices.RequireDeviceAsync(owner.User.Id, fresh.DeviceId, ct)).Id);
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.RequireDeviceAsync(owner.User.Id, owner.Device.Id, ct));
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
            new(nextDevice.DeviceId, "Next", Convert.ToBase64String(nextDevice.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof), TestContext.Current.CancellationToken), Wire.Json);
        var requestId = init.GetProperty("requestId").GetGuid();
        var issued = afterExpiry ? owner.Device.ExpiresAtMs.Value : owner.Device.IssuedAtMs - 1;
        var certificate = nextDevice.CertificateBody(owner.Device.Id, issued);
        var list = DeviceFixture.ListBody(owner.User.Id, 2,
            [(owner.Device.Id, owner.Device.Id), (nextDevice.DeviceId, owner.Device.Id)], owner.Device.Id, now);
        var error = await Assert.ThrowsAsync<ApiException>(() => store.Devices.ApproveLinkAsync(owner.User.Id, owner.Device,
            new(requestId, Convert.ToBase64String(certificate), owner.Fixture.SignedCertificate(certificate), Convert.ToBase64String(list), owner.Fixture.SignedList(list), DeviceFixture.LinkProof), TestContext.Current.CancellationToken));
        Assert.Equal(400, error.Status);
        Assert.Contains("signer's validity", error.Message);
        Assert.Equal(1, (await store.Db.DeviceLists.SingleAsync(TestContext.Current.CancellationToken)).Generation);
        Assert.False(await store.Db.Devices.AnyAsync(x => x.Id == nextDevice.DeviceId, TestContext.Current.CancellationToken));
        Assert.Equal("pending", (await store.Db.DeviceLinks.SingleAsync(TestContext.Current.CancellationToken)).State);
        certificate = nextDevice.CertificateBody(owner.Device.Id, now);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Device,
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
            new(second.DeviceId, "Second", Convert.ToBase64String(second.SigningKey), DeviceFixture.LinkNonce, DeviceFixture.LinkProof), TestContext.Current.CancellationToken), Wire.Json);
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var future = now + 120_000;
        var certificate = second.CertificateBody(owner.Device.Id, future);
        var list = DeviceFixture.ListBody(owner.User.Id, 2,
            [(owner.Device.Id, owner.Device.Id), (second.DeviceId, owner.Device.Id)], owner.Device.Id, future);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Device,
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
        Assert.Equal("device", new DeviceCertificateParser().Parse(bytes).DeviceId);
        Assert.Throws<DeviceCertificateFormatException>(() => new DeviceCertificateParser().Parse([.. bytes, 0]));
        var sig = fixture.Sign(Proofs.Tagged(DomainTags.DeviceCertV3, bytes));
        var verifier = new SignatureVerifier(); Assert.True(verifier.Verify(fixture.SigningKey, Proofs.Tagged(DomainTags.DeviceCertV3, bytes), sig));
        bytes[^1] ^= 1; Assert.False(verifier.Verify(fixture.SigningKey, Proofs.Tagged(DomainTags.DeviceCertV3, bytes), sig));
    }

    [Fact]
    public async Task ARecoveryKeyApprovesANewDeviceAndAnAccountHasOnlyOne()
    {
        var ct = TestContext.Current.CancellationToken;
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var recovery = new DeviceFixture(owner.User.Id, "recovery-entry");
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        DeviceService.RecoveryKeyWrite Write(DeviceFixture entry, long generation, params (string Device, string Signer)[] known)
        {
            var cert = entry.CertificateBody(owner.Device.Id, now);
            var list = DeviceFixture.ListBody(owner.User.Id, generation, [.. known, (entry.DeviceId, owner.Device.Id)], owner.Device.Id, now + generation);
            return new(Convert.ToBase64String(cert), owner.Fixture.SignedCertificate(cert), Convert.ToBase64String(list), owner.Fixture.SignedList(list), Convert.ToBase64String(new byte[64]));
        }
        await store.Devices.AddRecoveryKeyAsync(owner.User.Id, owner.Device, Write(recovery, 2, (owner.Device.Id, owner.Device.Id)), ct);
        var box = JsonSerializer.SerializeToElement(await store.Devices.RecoveryBoxAsync(owner.User.Id, ct), Wire.Json);
        Assert.Equal(recovery.DeviceId, box.GetProperty("deviceId").GetString());
        Assert.Equal(recovery.DeviceId, (await store.Devices.RequireRecoveryDeviceAsync(owner.User.Id, recovery.DeviceId, ct)).Id);
        Assert.Equal(404, (await Assert.ThrowsAsync<ApiException>(() => store.Devices.RequireRecoveryDeviceAsync(owner.User.Id, owner.Device.Id, ct))).Status);
        var challenge = JsonSerializer.SerializeToElement(store.Devices.CreateChallenge(owner.User.Id), Wire.Json).GetProperty("challengeId").GetGuid();
        var session = await Assert.ThrowsAsync<ApiException>(() => store.Devices.OpenSessionAsync(owner.User.Id,
            new(recovery.DeviceId, challenge, Convert.ToBase64String(new byte[3309])), ct));
        Assert.Equal(403, session.Status);
        Assert.Contains("recovery key", session.Message);
        var second = await Assert.ThrowsAsync<ApiException>(() => store.Devices.AddRecoveryKeyAsync(owner.User.Id, owner.Device,
            Write(new DeviceFixture(owner.User.Id, "second-recovery"), 3, (owner.Device.Id, owner.Device.Id), (recovery.DeviceId, owner.Device.Id)), ct));
        Assert.Equal(409, second.Status);

        var next = new DeviceFixture(owner.User.Id, "new-computer");
        var cert = next.CertificateBody(recovery.DeviceId, now + 10);
        var list = DeviceFixture.ListBody(owner.User.Id, 3,
            [(owner.Device.Id, owner.Device.Id), (recovery.DeviceId, owner.Device.Id), (next.DeviceId, recovery.DeviceId)], recovery.DeviceId, now + 10);
        DeviceService.RegisterDevice Request(DeviceFixture signer)
        {
            var challenge = JsonSerializer.SerializeToElement(store.Devices.CreateChallenge(owner.User.Id), Wire.Json);
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
}
