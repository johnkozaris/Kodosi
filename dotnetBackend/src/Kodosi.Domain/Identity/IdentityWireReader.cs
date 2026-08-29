using System.Buffers.Binary;
using System.Text;

namespace Kodosi.Domain;

internal ref struct IdentityWireReader
{
    private static readonly UTF8Encoding StrictUtf8 = new(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private readonly ReadOnlySpan<byte> _bytes;
    private readonly Func<string, Exception> _exceptionFactory;
    private int _position;

    public IdentityWireReader(
        ReadOnlySpan<byte> bytes,
        Func<string, Exception> exceptionFactory)
    {
        _bytes = bytes;
        _exceptionFactory = exceptionFactory;
        _position = 0;
    }

    public uint ReadUInt32BigEndian()
    {
        EnsureAvailable(4, "u32");
        var value = BinaryPrimitives.ReadUInt32BigEndian(_bytes.Slice(_position, 4));
        _position += 4;
        return value;
    }

    public ulong ReadUInt64BigEndian()
    {
        EnsureAvailable(8, "u64");
        var value = BinaryPrimitives.ReadUInt64BigEndian(_bytes.Slice(_position, 8));
        _position += 8;
        return value;
    }

    public long ReadNonNegativeInt64(string fieldName)
    {
        var raw = ReadUInt64BigEndian();
        return ToNonNegativeInt64(raw, fieldName);
    }

    public long ToNonNegativeInt64(ulong raw, string fieldName)
    {
        if (raw > long.MaxValue)
        {
            throw _exceptionFactory($"{fieldName} exceeds long.MaxValue ({raw})");
        }
        return (long)raw;
    }

    public long ReadUnixTimeMilliseconds(string fieldName)
    {
        var value = ReadNonNegativeInt64(fieldName);
        if (value > IdentityWireFormat.MaxUnixTimeMilliseconds)
        {
            throw _exceptionFactory(
                $"{fieldName} exceeds the maximum representable Unix timestamp ({value})");
        }
        return value;
    }

    public long ToUnixTimeMilliseconds(ulong raw, string fieldName)
    {
        var value = ToNonNegativeInt64(raw, fieldName);
        if (value > IdentityWireFormat.MaxUnixTimeMilliseconds)
        {
            throw _exceptionFactory(
                $"{fieldName} exceeds the maximum representable Unix timestamp ({value})");
        }
        return value;
    }

    public byte[] ReadLengthPrefixedBytes()
    {
        var length = ReadUInt32BigEndian();
        if (length > IdentityWireFormat.MaxFieldLength)
        {
            throw _exceptionFactory(
                $"field length {length} exceeds MaxFieldLength ({IdentityWireFormat.MaxFieldLength})");
        }
        EnsureAvailable((int)length, "length-prefixed bytes");
        var value = _bytes.Slice(_position, (int)length).ToArray();
        _position += (int)length;
        return value;
    }

    public string ReadLengthPrefixedString()
    {
        try
        {
            return StrictUtf8.GetString(ReadLengthPrefixedBytes());
        }
        catch (DecoderFallbackException)
        {
            throw _exceptionFactory("invalid UTF-8 in string field");
        }
    }

    public void ExpectConsumed()
    {
        if (_position != _bytes.Length)
        {
            throw _exceptionFactory(
                $"trailing bytes after parse: {_bytes.Length - _position} unconsumed");
        }
    }

    private void EnsureAvailable(int needed, string description)
    {
        if (_position + needed > _bytes.Length)
        {
            throw _exceptionFactory(
                $"truncated {description}: needed {needed} at offset {_position}, have {_bytes.Length - _position}");
        }
    }
}
