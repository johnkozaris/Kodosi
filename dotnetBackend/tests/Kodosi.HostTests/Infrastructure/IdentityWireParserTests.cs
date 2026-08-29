using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Infrastructure.Crypto;

namespace Kodosi.HostTests;

public sealed class IdentityWireParserTests
{
    [Fact]
    public void DeviceCertificateParser_Reads_Canonical_Binary_Format()
    {
        using var body = new MemoryStream();
        IdentityWireTestWriter.WriteString(body, Guid.NewGuid().ToString());
        IdentityWireTestWriter.WriteString(body, "device-1");
        IdentityWireTestWriter.WriteString(body, "Laptop");
        IdentityWireTestWriter.WriteString(body, "device-1");
        IdentityWireTestWriter.WriteBytes(body, new byte[1184]);
        IdentityWireTestWriter.WriteBytes(body, new byte[1952]);
        IdentityWireTestWriter.WriteUInt64(body, 1_700_000_000_000);
        IdentityWireTestWriter.WriteUInt64(body, 0);

        var parsed = new DeviceCertificateParser().Parse(body.ToArray());

        Assert.Equal("device-1", parsed.DeviceId);
        Assert.Equal("Laptop", parsed.DeviceLabel);
        Assert.True(parsed.IsSelfSigned);
        Assert.Null(parsed.ExpiresAtMs);
    }

    [Theory]
    [InlineData(" Laptop")]
    [InlineData("Laptop ")]
    public void DeviceCertificateParser_Rejects_Noncanonical_Label(string label)
    {
        var userId = UserId.New();
        var body = TestDeviceCertificate.Body(
            userId,
            "device-1",
            label,
            "device-1",
            new byte[1184],
            new byte[1952],
            1,
            null);

        Assert.Throws<DeviceCertificateFormatException>(() =>
            new DeviceCertificateParser().Parse(body));
    }

    [Fact]
    public void SignedDeviceListParser_Rejects_Expiry_Not_After_Issue()
    {
        var userId = UserId.New();
        var body = TestDeviceList.Body(
            userId,
            1,
            TestDeviceList.Entries([("device-1", "device-1")]),
            "device-1",
            issuedAtMs: 2,
            expiresAtMs: 1);

        Assert.Throws<SignedDeviceListFormatException>(() =>
            new SignedDeviceListParser().Parse(body));
    }

    [Fact]
    public void SignedDeviceListParser_Reads_Canonical_Binary_Format()
    {
        using var body = new MemoryStream();
        IdentityWireTestWriter.WriteString(body, Guid.NewGuid().ToString());
        IdentityWireTestWriter.WriteUInt64(body, 1);
        IdentityWireTestWriter.WriteUInt32(body, 1);
        IdentityWireTestWriter.WriteString(body, "device-1");
        IdentityWireTestWriter.WriteString(body, "device-1");
        IdentityWireTestWriter.WriteString(body, "device-1");
        IdentityWireTestWriter.WriteUInt64(body, 1_700_000_000_000);
        IdentityWireTestWriter.WriteUInt64(body, 0);

        var parsed = new SignedDeviceListParser().Parse(body.ToArray());

        Assert.Equal(1, parsed.Generation);
        Assert.Single(parsed.Entries);
        Assert.True(parsed.Entries[0].IsSelfSigned);
        Assert.Null(parsed.ExpiresAtMs);
    }

    [Fact]
    public void SignedDeviceListParser_Rejects_Duplicate_Devices()
    {
        var userId = UserId.New();
        var body = TestDeviceList.ExactBody(
            userId,
            1,
            TestDeviceList.Entries([
                ("device-1", "device-1"),
                ("device-1", "device-1")]),
            "device-1",
            issuedAtMs: 1,
            expiresAtMs: null);

        Assert.Throws<SignedDeviceListFormatException>(() =>
            new SignedDeviceListParser().Parse(body));
    }

    [Fact]
    public void SignedDeviceListParser_Rejects_Signer_Outside_Entries()
    {
        var userId = UserId.New();
        var body = TestDeviceList.ExactBody(
            userId,
            1,
            TestDeviceList.Entries([("device-1", "device-1")]),
            "device-2",
            issuedAtMs: 1,
            expiresAtMs: null);

        Assert.Throws<SignedDeviceListFormatException>(() =>
            new SignedDeviceListParser().Parse(body));
    }

    [Theory]
    [InlineData(0UL)]
    [InlineData(9223372036854775808UL)]
    public void SignedDeviceListParser_Rejects_Unrepresentable_Generation(ulong generation)
    {
        using var body = new MemoryStream();
        IdentityWireTestWriter.WriteString(body, Guid.NewGuid().ToString());
        IdentityWireTestWriter.WriteUInt64(body, generation);
        IdentityWireTestWriter.WriteUInt32(body, 1);
        IdentityWireTestWriter.WriteString(body, "device-1");
        IdentityWireTestWriter.WriteString(body, "device-1");
        IdentityWireTestWriter.WriteString(body, "device-1");
        IdentityWireTestWriter.WriteUInt64(body, 1);
        IdentityWireTestWriter.WriteUInt64(body, 0);

        Assert.Throws<SignedDeviceListFormatException>(() =>
            new SignedDeviceListParser().Parse(body.ToArray()));
    }

    [Fact]
    public void SignedDeviceListParser_Rejects_Trailing_Bytes()
    {
        var body = TestDeviceList.Body(
            UserId.New(),
            1,
            TestDeviceList.Entries([("device-1", "device-1")]),
            "device-1",
            issuedAtMs: 1,
            expiresAtMs: null);

        Assert.Throws<SignedDeviceListFormatException>(() =>
            new SignedDeviceListParser().Parse([.. body, 0]));
    }

    [Fact]
    public void Shared_Wire_Reader_Preserves_Parser_Specific_Exceptions()
    {
        Assert.Throws<DeviceCertificateFormatException>(() =>
            new DeviceCertificateParser().Parse([0, 0, 0, 1, 0xFF]));
        Assert.Throws<SignedDeviceListFormatException>(() =>
            new SignedDeviceListParser().Parse([0, 0, 0, 1, 0xFF]));
    }

    [Fact]
    public void DeviceCertificateParser_Rejects_Unsigned_Timestamp_Overflow_As_Format_Error()
    {
        using var body = new MemoryStream();
        IdentityWireTestWriter.WriteString(body, Guid.NewGuid().ToString());
        IdentityWireTestWriter.WriteString(body, "device-1");
        IdentityWireTestWriter.WriteString(body, "Laptop");
        IdentityWireTestWriter.WriteString(body, "device-1");
        IdentityWireTestWriter.WriteBytes(body, [1]);
        IdentityWireTestWriter.WriteBytes(body, [2]);
        IdentityWireTestWriter.WriteUInt64(body, 1);
        IdentityWireTestWriter.WriteUInt64(body, ulong.MaxValue);

        Assert.Throws<DeviceCertificateFormatException>(() =>
            new DeviceCertificateParser().Parse(body.ToArray()));
    }

    [Fact]
    public void IdentityParsers_Reject_Unrepresentable_Unix_Timestamps()
    {
        var overflow = checked((ulong)IdentityWireFormat.MaxUnixTimeMilliseconds + 1);

        using var certificate = new MemoryStream();
        IdentityWireTestWriter.WriteString(certificate, Guid.NewGuid().ToString());
        IdentityWireTestWriter.WriteString(certificate, "device-1");
        IdentityWireTestWriter.WriteString(certificate, "Laptop");
        IdentityWireTestWriter.WriteString(certificate, "device-1");
        IdentityWireTestWriter.WriteBytes(certificate, [1]);
        IdentityWireTestWriter.WriteBytes(certificate, [2]);
        IdentityWireTestWriter.WriteUInt64(certificate, overflow);
        IdentityWireTestWriter.WriteUInt64(certificate, 0);

        using var list = new MemoryStream();
        IdentityWireTestWriter.WriteString(list, Guid.NewGuid().ToString());
        IdentityWireTestWriter.WriteUInt64(list, 1);
        IdentityWireTestWriter.WriteUInt32(list, 1);
        IdentityWireTestWriter.WriteString(list, "device-1");
        IdentityWireTestWriter.WriteString(list, "device-1");
        IdentityWireTestWriter.WriteString(list, "device-1");
        IdentityWireTestWriter.WriteUInt64(list, 1);
        IdentityWireTestWriter.WriteUInt64(list, overflow);

        Assert.Throws<DeviceCertificateFormatException>(() =>
            new DeviceCertificateParser().Parse(certificate.ToArray()));
        Assert.Throws<SignedDeviceListFormatException>(() =>
            new SignedDeviceListParser().Parse(list.ToArray()));
    }

    [Theory]
    [InlineData(" device-1 ")]
    [InlineData("   ")]
    public void IdentityParsers_Reject_Noncanonical_Device_Ids(string deviceId)
    {
        using var certificate = new MemoryStream();
        IdentityWireTestWriter.WriteString(certificate, Guid.NewGuid().ToString());
        IdentityWireTestWriter.WriteString(certificate, deviceId);
        IdentityWireTestWriter.WriteString(certificate, "Laptop");
        IdentityWireTestWriter.WriteString(certificate, deviceId);
        IdentityWireTestWriter.WriteBytes(certificate, [1]);
        IdentityWireTestWriter.WriteBytes(certificate, [2]);
        IdentityWireTestWriter.WriteUInt64(certificate, 1);
        IdentityWireTestWriter.WriteUInt64(certificate, 0);

        using var list = new MemoryStream();
        IdentityWireTestWriter.WriteString(list, Guid.NewGuid().ToString());
        IdentityWireTestWriter.WriteUInt64(list, 1);
        IdentityWireTestWriter.WriteUInt32(list, 1);
        IdentityWireTestWriter.WriteString(list, deviceId);
        IdentityWireTestWriter.WriteString(list, deviceId);
        IdentityWireTestWriter.WriteString(list, deviceId);
        IdentityWireTestWriter.WriteUInt64(list, 1);
        IdentityWireTestWriter.WriteUInt64(list, 0);

        Assert.Throws<DeviceCertificateFormatException>(() =>
            new DeviceCertificateParser().Parse(certificate.ToArray()));
        Assert.Throws<SignedDeviceListFormatException>(() =>
            new SignedDeviceListParser().Parse(list.ToArray()));
    }

    [Fact]
    public void DeviceListEntrySet_Requires_Stored_Certificate_Signer_Continuity()
    {
        var userId = UserId.New();
        var device = TestDeviceCertificate.CreateDevice(
            userId,
            "device-2",
            kemPublicKey: new byte[1184],
            signingPublicKey: new byte[1952],
            deviceLabel: "Device",
            signerDeviceId: "device-retired",
            issuedAt: DateTimeOffset.UtcNow,
            expiresAt: null);
        var devices = new Dictionary<string, UserDevice>(StringComparer.Ordinal)
        {
            [device.DeviceId] = device,
        };

        DeviceListEntrySet.RequireCertificateSignerContinuity(
            [(device.DeviceId, "device-retired")],
            devices);
        Assert.Throws<DeviceEnrollmentException>(() =>
            DeviceListEntrySet.RequireCertificateSignerContinuity(
                [(device.DeviceId, device.DeviceId)],
                devices));
    }
}
