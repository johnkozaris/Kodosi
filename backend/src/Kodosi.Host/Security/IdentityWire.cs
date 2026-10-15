using System.Buffers.Binary;
using System.Diagnostics.CodeAnalysis;
using System.Text;

namespace Kodosi.Security;

public static class IdentityWireFormat
{
    public const int MaxEntries = 256;
    public const int MaxDeviceCertificateBodyLength = 4_584;
    public const int MaxSignedDeviceListBodyLength = 527_432;
    public const int MlDsa65PublicKeyLength = 1_952;
    public const int MlDsa65SignatureLength = 3_309;
}

public sealed class IdentityFormatException(string message) : Exception(message);

internal static class DeviceIdRules
{
    public static bool IsCanonical([NotNullWhen(true)] string? value) =>
        !string.IsNullOrWhiteSpace(value) && value.Length <= 256 && value == value.Trim() && !value.Any(char.IsControl);

    public static string Require(string? value) =>
        IsCanonical(value) ? value : throw ApiException.Invalid("Device ID must be canonical.");
}

public sealed record DeviceCertificate(string UserId, string DeviceId, string DeviceLabel, string SignerDeviceId,
    byte[] SigPublicKey, long IssuedAtMs, long? ExpiresAtMs)
{
    public bool IsSelfSigned => SignerDeviceId == DeviceId;

    public static DeviceCertificate Parse(ReadOnlySpan<byte> body)
    {
        var reader = new IdentityWireReader(body, IdentityWireFormat.MaxDeviceCertificateBodyLength);
        var userId = reader.ReadUserId();
        var deviceId = reader.ReadDeviceId();
        var label = reader.ReadString();
        if (string.IsNullOrWhiteSpace(label) || label.Length > 128 || label != label.Trim())
            throw new IdentityFormatException("device_label must be canonical text of at most 128 UTF-16 code units");
        var signerDeviceId = reader.ReadDeviceId();
        var key = reader.ReadBytes();
        if (key.Length != IdentityWireFormat.MlDsa65PublicKeyLength)
            throw new IdentityFormatException("sig_public_key has the wrong length");
        var (issuedAtMs, expiresAtMs) = reader.ReadValidity();
        reader.ExpectEnd();
        return new(userId, deviceId, label, signerDeviceId, key, issuedAtMs, expiresAtMs);
    }
}

public sealed record SignedDeviceList(string UserId, long Generation, IReadOnlyList<SignedDeviceList.Entry> Entries,
    string SignerDeviceId, long IssuedAtMs, long? ExpiresAtMs)
{
    public sealed record Entry(string DeviceId, string SignerDeviceId);

    public static SignedDeviceList Parse(ReadOnlySpan<byte> body)
    {
        var reader = new IdentityWireReader(body, IdentityWireFormat.MaxSignedDeviceListBodyLength);
        var userId = reader.ReadUserId();
        var generation = reader.ReadInt64();
        if (generation < 1) throw new IdentityFormatException("generation must be at least 1");
        var count = reader.ReadUInt32();
        if (count > IdentityWireFormat.MaxEntries) throw new IdentityFormatException("the list has too many entries");
        var entries = new List<Entry>((int)count);
        for (var i = 0; i < count; i++) entries.Add(new(reader.ReadDeviceId(), reader.ReadDeviceId()));
        var signer = reader.ReadDeviceId();
        var (issuedAtMs, expiresAtMs) = reader.ReadValidity();
        reader.ExpectEnd();
        if (entries.DistinctBy(x => x.DeviceId).Count() != entries.Count) throw new IdentityFormatException("duplicate device_id");
        if (!entries.Any(x => x.DeviceId == signer)) throw new IdentityFormatException("the list signer is not in the entries");
        return new(userId, generation, entries, signer, issuedAtMs, expiresAtMs);
    }
}

internal ref struct IdentityWireReader
{
    private const uint MaxFieldLength = 65_536;
    private const long MaxUnixTimeMilliseconds = 253_402_300_799_999;
    private static readonly UTF8Encoding StrictUtf8 = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);
    private readonly ReadOnlySpan<byte> bytes;
    private int position;

    public IdentityWireReader(ReadOnlySpan<byte> bytes, int maxLength)
    {
        if (bytes.IsEmpty || bytes.Length > maxLength) throw new IdentityFormatException("body length is invalid");
        this.bytes = bytes;
    }

    public uint ReadUInt32() => BinaryPrimitives.ReadUInt32BigEndian(Take(4));

    public long ReadInt64()
    {
        var value = BinaryPrimitives.ReadUInt64BigEndian(Take(8));
        return value <= long.MaxValue ? (long)value : throw new IdentityFormatException("integer is too large");
    }

    public byte[] ReadBytes()
    {
        var length = ReadUInt32();
        if (length > MaxFieldLength) throw new IdentityFormatException("field is too long");
        return Take((int)length).ToArray();
    }

    public string ReadString()
    {
        var value = ReadBytes();
        try { return StrictUtf8.GetString(value); }
        catch (DecoderFallbackException) { throw new IdentityFormatException("invalid UTF-8 in string field"); }
    }

    public string ReadUserId()
    {
        var value = ReadString();
        return Guid.TryParseExact(value, "D", out var id) && value == id.ToString("D")
            ? value : throw new IdentityFormatException("user_id must be a canonical lowercase UUID");
    }

    public string ReadDeviceId()
    {
        var value = ReadString();
        return DeviceIdRules.IsCanonical(value) ? value : throw new IdentityFormatException("device ID must be canonical");
    }

    public (long IssuedAtMs, long? ExpiresAtMs) ReadValidity()
    {
        var issued = ReadTime();
        var expires = ReadTime();
        if (expires == 0) return (issued, null);
        return expires > issued ? (issued, expires) : throw new IdentityFormatException("expires_at_ms must be after issued_at_ms");
    }

    public readonly void ExpectEnd()
    {
        if (position != bytes.Length) throw new IdentityFormatException("trailing bytes after parse");
    }

    private long ReadTime()
    {
        var value = ReadInt64();
        return value <= MaxUnixTimeMilliseconds ? value : throw new IdentityFormatException("timestamp is too large");
    }

    private ReadOnlySpan<byte> Take(int length)
    {
        if (length > bytes.Length - position) throw new IdentityFormatException("truncated body");
        var value = bytes.Slice(position, length);
        position += length;
        return value;
    }
}
