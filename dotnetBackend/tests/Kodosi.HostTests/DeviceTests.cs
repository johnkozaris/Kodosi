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
