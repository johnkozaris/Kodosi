using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.Devices;
using Kodosi.Realtime;
using Kodosi.Security;
using Microsoft.EntityFrameworkCore;
using Xunit;

namespace Kodosi.HostTests;

[Collection("PostgreSQL")]
public sealed class DeviceTests(PostgresFixture postgres)
{
    [Fact]
    public async Task DeviceRemovalPreservesOtherDevicesAndInvalidatesKeys()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var second = new DeviceFixture(owner.User.Id, "second-device");
        var init = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.KemKey), Convert.ToBase64String(second.SigningKey)), TestContext.Current.CancellationToken), Wire.Json);
        var retry = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.KemKey), Convert.ToBase64String(second.SigningKey)), TestContext.Current.CancellationToken), Wire.Json);
        Assert.Equal(init.GetProperty("userCode").GetString(), retry.GetProperty("userCode").GetString());
        var code = init.GetProperty("userCode").GetString()!;
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var cert = second.CertificateBody(owner.Device.Id, now);
        var next = DeviceFixture.ListBody(owner.User.Id, 2, [(owner.Device.Id, owner.Device.Id), (second.DeviceId, owner.Device.Id)], owner.Device.Id, now);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Device,
            new(code, Convert.ToBase64String(cert), owner.Fixture.SignedCertificate(cert), Convert.ToBase64String(next), owner.Fixture.SignedList(next)), TestContext.Current.CancellationToken);
        Assert.Equal(second.DeviceId, (await store.Devices.RequireDeviceAsync(owner.User.Id, second.DeviceId, TestContext.Current.CancellationToken)).Id);
        var removed = DeviceFixture.ListBody(owner.User.Id, 3, [(owner.Device.Id, owner.Device.Id)], owner.Device.Id, now + 1);
        await store.Devices.ReplaceListAsync(owner.User.Id, new(Convert.ToBase64String(removed), owner.Fixture.SignedList(removed)), TestContext.Current.CancellationToken);
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.RequireDeviceAsync(owner.User.Id, second.DeviceId, TestContext.Current.CancellationToken));
        Assert.Equal(owner.Device.Id, (await store.Devices.RequireDeviceAsync(owner.User.Id, owner.Device.Id, TestContext.Current.CancellationToken)).Id);
        var readd = DeviceFixture.ListBody(owner.User.Id, 4, [(owner.Device.Id, owner.Device.Id), (second.DeviceId, owner.Device.Id)], owner.Device.Id, now + 2);
        await Assert.ThrowsAsync<ApiException>(() => store.Devices.ReplaceListAsync(owner.User.Id, new(Convert.ToBase64String(readd), owner.Fixture.SignedList(readd)), TestContext.Current.CancellationToken));
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
        Assert.NotNull(await store.Devices.IdentityAsync(owner.User.Id, owner.User.Id, TestContext.Current.CancellationToken));
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
            new(nextDevice.DeviceId, "Next", Convert.ToBase64String(nextDevice.KemKey), Convert.ToBase64String(nextDevice.SigningKey)), TestContext.Current.CancellationToken), Wire.Json);
        var code = init.GetProperty("userCode").GetString()!;
        var issued = afterExpiry ? owner.Device.ExpiresAtMs.Value : owner.Device.IssuedAtMs - 1;
        var certificate = nextDevice.CertificateBody(owner.Device.Id, issued);
        var list = DeviceFixture.ListBody(owner.User.Id, 2,
            [(owner.Device.Id, owner.Device.Id), (nextDevice.DeviceId, owner.Device.Id)], owner.Device.Id, now);
        var error = await Assert.ThrowsAsync<ApiException>(() => store.Devices.ApproveLinkAsync(owner.User.Id, owner.Device,
            new(code, Convert.ToBase64String(certificate), owner.Fixture.SignedCertificate(certificate), Convert.ToBase64String(list), owner.Fixture.SignedList(list)), TestContext.Current.CancellationToken));
        Assert.Equal(400, error.Status);
        Assert.Contains("signer's validity", error.Message);
        Assert.Equal(1, (await store.Db.DeviceLists.SingleAsync(TestContext.Current.CancellationToken)).Generation);
        Assert.False(await store.Db.Devices.AnyAsync(x => x.Id == nextDevice.DeviceId, TestContext.Current.CancellationToken));
        Assert.Equal("pending", (await store.Db.DeviceLinks.SingleAsync(TestContext.Current.CancellationToken)).State);
        certificate = nextDevice.CertificateBody(owner.Device.Id, now);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Device,
            new(code, Convert.ToBase64String(certificate), owner.Fixture.SignedCertificate(certificate), Convert.ToBase64String(list), owner.Fixture.SignedList(list)), TestContext.Current.CancellationToken);
        Assert.NotNull(await store.Devices.RequireDeviceAsync(owner.User.Id, nextDevice.DeviceId, TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task PredecessorAwareIssuanceAllowsRemovalWithinClockTolerance()
    {
        await using var store = await TestStore.CreateAsync(postgres);
        var owner = await store.UserAsync("owner");
        var second = new DeviceFixture(owner.User.Id, "second-device");
        var init = JsonSerializer.SerializeToElement(await store.Devices.StartLinkAsync(owner.User.Id,
            new(second.DeviceId, "Second", Convert.ToBase64String(second.KemKey), Convert.ToBase64String(second.SigningKey)), TestContext.Current.CancellationToken), Wire.Json);
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var future = now + 120_000;
        var certificate = second.CertificateBody(owner.Device.Id, future);
        var list = DeviceFixture.ListBody(owner.User.Id, 2,
            [(owner.Device.Id, owner.Device.Id), (second.DeviceId, owner.Device.Id)], owner.Device.Id, future);
        await store.Devices.ApproveLinkAsync(owner.User.Id, owner.Device,
            new(init.GetProperty("userCode").GetString()!, Convert.ToBase64String(certificate), owner.Fixture.SignedCertificate(certificate), Convert.ToBase64String(list), owner.Fixture.SignedList(list)), TestContext.Current.CancellationToken);
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
        var sig = fixture.Sign(Proofs.Tagged(DomainTags.DeviceCertV2, bytes));
        var verifier = new SignatureVerifier(); Assert.True(verifier.Verify(fixture.SigningKey, Proofs.Tagged(DomainTags.DeviceCertV2, bytes), sig));
        bytes[^1] ^= 1; Assert.False(verifier.Verify(fixture.SigningKey, Proofs.Tagged(DomainTags.DeviceCertV2, bytes), sig));
    }
}
