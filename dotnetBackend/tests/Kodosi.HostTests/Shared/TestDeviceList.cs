using System.Buffers.Binary;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using Kodosi.Domain;

namespace Kodosi.HostTests;

internal static class TestDeviceList
{
    private static readonly UTF8Encoding Utf8 = new(false, true);

    public static string Entries(
        IEnumerable<(string DeviceId, string SignerDeviceId)> entries) =>
        JsonSerializer.Serialize(entries.Select(entry =>
            new TestEntry(entry.DeviceId, entry.SignerDeviceId)));

    public static byte[] Body(
        UserId userId,
        long generation,
        string deviceIdsJson,
        string signerDeviceId,
        long issuedAtMs,
        long? expiresAtMs) =>
        Body(userId, generation, deviceIdsJson, signerDeviceId, issuedAtMs, expiresAtMs, true);

    public static byte[] ExactBody(
        UserId userId,
        long generation,
        string deviceIdsJson,
        string signerDeviceId,
        long issuedAtMs,
        long? expiresAtMs) =>
        Body(userId, generation, deviceIdsJson, signerDeviceId, issuedAtMs, expiresAtMs, false);

    private static byte[] Body(
        UserId userId,
        long generation,
        string deviceIdsJson,
        string signerDeviceId,
        long issuedAtMs,
        long? expiresAtMs,
        bool includeSigner)
    {
        var entries = JsonSerializer.Deserialize<List<TestEntry?>>(deviceIdsJson)
            ?.Where(entry => entry is not null)
            .Cast<TestEntry>()
            .ToList()
            ?? throw new InvalidOperationException("Device-list fixture entries are required.");
        if (includeSigner && entries.All(entry => entry.DeviceId != signerDeviceId))
        {
            entries.Add(new TestEntry(signerDeviceId, signerDeviceId));
        }
        using var canonical = new MemoryStream();
        WriteString(canonical, userId.Value.ToString());
        WriteUInt64(canonical, checked((ulong)generation));
        WriteUInt32(canonical, checked((uint)entries.Count));
        foreach (var entry in entries)
        {
            WriteString(canonical, entry.DeviceId);
            WriteString(canonical, entry.SignerDeviceId);
        }
        WriteString(canonical, signerDeviceId);
        WriteUInt64(canonical, checked((ulong)issuedAtMs));
        WriteUInt64(canonical, checked((ulong)(expiresAtMs ?? 0)));
        return canonical.ToArray();
    }

    public static UserDeviceList Create(
        UserId userId,
        long generation,
        string deviceIdsJson,
        string signerDeviceId,
        byte[] signature,
        long issuedAtMs,
        long? expiresAtMs)
    {
        var canonical = Body(
            userId,
            generation,
            deviceIdsJson,
            signerDeviceId,
            issuedAtMs,
            expiresAtMs);
        var canonicalSignature = new byte[IdentityWireFormat.MlDsa65SignatureLength];
        signature.CopyTo(canonicalSignature, 0);
        return UserDeviceList.Create(userId, generation, canonical, canonicalSignature);
    }

    private static void WriteString(Stream stream, string value)
    {
        var bytes = Utf8.GetBytes(value);
        WriteUInt32(stream, checked((uint)bytes.Length));
        stream.Write(bytes);
    }

    private static void WriteUInt32(Stream stream, uint value)
    {
        Span<byte> bytes = stackalloc byte[4];
        BinaryPrimitives.WriteUInt32BigEndian(bytes, value);
        stream.Write(bytes);
    }

    private static void WriteUInt64(Stream stream, ulong value)
    {
        Span<byte> bytes = stackalloc byte[8];
        BinaryPrimitives.WriteUInt64BigEndian(bytes, value);
        stream.Write(bytes);
    }

    private sealed record TestEntry(
        [property: JsonPropertyName("deviceId")] string DeviceId,
        [property: JsonPropertyName("signerDeviceId")] string SignerDeviceId);
}
