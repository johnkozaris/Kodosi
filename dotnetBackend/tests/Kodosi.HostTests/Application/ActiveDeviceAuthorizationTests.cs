using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class ActiveDeviceAuthorizationTests
{
    [Fact]
    public void IsAuthorized_Requires_CurrentCertificateAndSignedListMembership()
    {
        var userId = UserId.New();
        var now = DateTimeOffset.UtcNow;
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            "device-1",
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Laptop",
            signerDeviceId: "device-1",
            issuedAt: now.AddMinutes(-1),
            expiresAt: now.AddMinutes(1));
        var listed = DeviceList(
            userId,
            """[{"deviceId":"device-1","signerDeviceId":"device-1"}]""",
            now.AddMinutes(1));
        var omitted = TestDeviceList.Create(
            userId,
            1,
            """[{"deviceId":"other","signerDeviceId":"other"}]""",
            "other",
            [2],
            now.AddMinutes(-2).ToUnixTimeMilliseconds(),
            now.AddMinutes(1).ToUnixTimeMilliseconds());

        Assert.True(ActiveDeviceAuthorization.IsAuthorized(device, listed, userId, now));
        Assert.False(ActiveDeviceAuthorization.IsAuthorized(device, omitted, userId, now));
        Assert.False(ActiveDeviceAuthorization.IsAuthorized(
            device,
            listed,
            userId,
            now.AddMinutes(2)));
    }

    [Fact]
    public void Evaluate_Surfaces_Persisted_Device_List_Corruption()
    {
        var userId = UserId.New();
        var now = DateTimeOffset.UtcNow;
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            "device-1",
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Laptop",
            signerDeviceId: "device-1",
            issuedAt: now.AddMinutes(-1),
            expiresAt: now.AddMinutes(1));
        var list = DeviceList(
            userId,
            """[{"deviceId":"device-1","signerDeviceId":"device-1"}]""",
            now.AddMinutes(1));
        DomainFixtureHydrator.SetDeviceListBody(list, [0]);

        Assert.Throws<DeviceListCorruptionException>(() =>
            ActiveDeviceAuthorization.Evaluate(device, list, userId, now));
    }

    [Fact]
    public void Evaluate_Uses_Earliest_Certificate_Or_List_Expiry()
    {
        var userId = UserId.New();
        var now = DateTimeOffset.UtcNow;
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            "device-1",
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Laptop",
            signerDeviceId: "device-1",
            issuedAt: now.AddMinutes(-1),
            expiresAt: now.AddMinutes(10));
        var list = DeviceList(
            userId,
            """[{"deviceId":"device-1","signerDeviceId":"device-1"}]""",
            now.AddMinutes(5));

        var decision = ActiveDeviceAuthorization.Evaluate(device, list, userId, now);

        Assert.True(decision.Authorized);
        Assert.Equal(
            now.AddMinutes(5).ToUnixTimeMilliseconds(),
            decision.ExpiresAt?.ToUnixTimeMilliseconds());
    }

    private static UserDeviceList DeviceList(
        UserId userId,
        string entries,
        DateTimeOffset? expiresAt) =>
        TestDeviceList.Create(
            userId,
            1,
            entries,
            "device-1",
            [2],
            DateTimeOffset.UtcNow.AddMinutes(-2).ToUnixTimeMilliseconds(),
            expiresAt?.ToUnixTimeMilliseconds());
}
