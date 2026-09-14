
namespace Kodosi.Security;



public sealed class SignedDeviceListParser
{
    private const ulong NoExpiry = IdentityWireFormat.NoExpirySentinel;

    public sealed record ListEntry(string DeviceId, string SignerDeviceId);

    public sealed record ParsedSignedDeviceList(
        string UserId,
        long Generation,
        IReadOnlyList<ListEntry> Entries,
        string SignerDeviceId,
        long IssuedAtMs,
        long? ExpiresAtMs);

    public ParsedSignedDeviceList Parse(ReadOnlySpan<byte> body)
    {
        if (body.IsEmpty || body.Length > IdentityWireFormat.MaxSignedDeviceListBodyLength)
        {
            throw new SignedDeviceListFormatException("signed list body length is invalid");
        }
        var cursor = new IdentityWireReader(
            body,
            static message => new SignedDeviceListFormatException(message));
        var userId = cursor.ReadLengthPrefixedString();
        if (!Guid.TryParseExact(userId, "D", out var parsedUserId)
            || !string.Equals(
                userId,
                parsedUserId.ToString("D"),
                StringComparison.Ordinal))
        {
            throw new SignedDeviceListFormatException(
                "user_id must be a canonical lowercase UUID");
        }

        var generation = cursor.ReadNonNegativeInt64("generation");
        var entryCount = cursor.ReadUInt32BigEndian();

        if (entryCount > IdentityWireFormat.MaxEntries)
        {
            throw new SignedDeviceListFormatException(
                $"entry_count {entryCount} exceeds MaxEntries ({IdentityWireFormat.MaxEntries})");
        }
        if (generation < 1)
        {
            throw new SignedDeviceListFormatException(
                $"generation must be >= 1, got {generation}");
        }

        var entries = new List<ListEntry>((int)entryCount);
        for (var i = 0; i < entryCount; i++)
        {
            var deviceId = ReadCanonicalDeviceId(ref cursor, "device_id");
            var signer = ReadCanonicalDeviceId(ref cursor, "entry signer_device_id");
            entries.Add(new ListEntry(deviceId, signer));
        }

        var listSigner = ReadCanonicalDeviceId(ref cursor, "list signer_device_id");
        var issuedAtMs = cursor.ReadUnixTimeMilliseconds("issued_at_ms");
        var expiresRaw = cursor.ReadUInt64BigEndian();
        cursor.ExpectConsumed();
        var expiresAtMs = expiresRaw == NoExpiry
            ? (long?)null
            : cursor.ToUnixTimeMilliseconds(expiresRaw, "expires_at_ms");
        if (expiresAtMs is { } expiry && expiry <= issuedAtMs)
        {
            throw new SignedDeviceListFormatException(
                "expires_at_ms must be greater than issued_at_ms");
        }



        var deviceIdSet = new HashSet<string>(entries.Select(e => e.DeviceId));
        if (deviceIdSet.Count != entries.Count)
        {
            throw new SignedDeviceListFormatException("duplicate device_id in list entries");
        }

        if (!deviceIdSet.Contains(listSigner))
        {
            throw new SignedDeviceListFormatException(
                $"list signer {listSigner} is not in the entries");
        }

        return new ParsedSignedDeviceList(
            userId,
            generation,
            entries,
            listSigner,
            issuedAtMs,
            expiresAtMs);
    }
    private static string ReadCanonicalDeviceId(
        ref IdentityWireReader cursor,
        string fieldName)
    {
        var value = cursor.ReadLengthPrefixedString();
        if (string.IsNullOrWhiteSpace(value)
            || value.Length > DeviceIdRules.MaximumLength
            || !string.Equals(value, value.Trim(), StringComparison.Ordinal))
        {
            throw new SignedDeviceListFormatException(
                $"{fieldName} must be a non-blank canonical device ID of at most {DeviceIdRules.MaximumLength} characters");
        }
        return value;
    }
}

public sealed class SignedDeviceListFormatException(string message) : Exception(message)
{
}
