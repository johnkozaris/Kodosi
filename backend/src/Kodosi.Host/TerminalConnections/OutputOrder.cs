using System.Buffers.Binary;

namespace Kodosi.TerminalConnections;

internal sealed class OutputOrder
{
    public const int MaximumFrame = 8 * 1024 * 1024 + 65_536;
    public const int MaximumNoticeFrame = 29 + 1 + 16_384 + 16;
    private ulong? rawCounter;
    private ulong? noticeCounter;
    public ulong? NextSequence { get; private set; }

    public void Clear()
    {
        rawCounter = null; noticeCounter = null; NextSequence = null;
    }

    public void Restart() => NextSequence = null;

    public void Accept(FrameHeader header)
    {
        if (header.Type == 5)
        {
            if (noticeCounter is { } latest && header.Counter <= latest) throw ApiException.Invalid("Terminal notice replay was rejected.");
            noticeCounter = header.Counter;
            return;
        }
        if (header.Type != 4) throw ApiException.Invalid("Unsupported terminal output frame.");
        if (rawCounter is { } counter && header.Counter <= counter) throw ApiException.Invalid("Terminal output replay was rejected.");
        if (header.First >= header.Next || NextSequence is { } seen && header.First < seen)
            throw ApiException.Invalid("Terminal output sequence went back.");
        rawCounter = header.Counter; NextSequence = header.Next;
    }

    public bool Follows(FrameHeader checkpoint) => NextSequence is not { } seen || checkpoint.Next >= seen;

    public static FrameHeader? Header(byte[] frame, int generation)
    {
        if (frame.Length < 45 || frame.Length > MaximumFrame || frame[0] is not (3 or 4 or 5)
            || frame[0] == 5 && frame.Length > MaximumNoticeFrame)
            throw ApiException.Invalid("Unsupported terminal frame.");
        var span = frame.AsSpan();
        var encoded = BinaryPrimitives.ReadUInt32BigEndian(span[1..5]);
        if (encoded > (uint)generation) throw ApiException.Conflict("Terminal key generation changed.");
        if (encoded < (uint)generation) return null;
        return new FrameHeader(frame[0], BinaryPrimitives.ReadUInt64BigEndian(span[5..13]),
            BinaryPrimitives.ReadUInt64BigEndian(span[13..21]), BinaryPrimitives.ReadUInt64BigEndian(span[21..29]));
    }
    public readonly record struct FrameHeader(byte Type, ulong Counter, ulong First, ulong Next);
}
