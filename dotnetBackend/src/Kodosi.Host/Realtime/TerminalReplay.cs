using System.Buffers.Binary;

namespace Kodosi.Realtime;

internal sealed class TerminalReplay
{
    public const int MaximumFrame = 8 * 1024 * 1024 + 65_536;
    public const int MaximumRawBytes = 8 * 1024 * 1024;
    private readonly Queue<RawFrame> raw = new();
    private int rawBytes;
    private ulong? checkpointCounter;
    private ulong? rawCounter;
    private ulong checkpointRevision;
    public ulong? NextSequence { get; private set; }

    public void Clear()
    {
        raw.Clear(); rawBytes = 0; checkpointCounter = null; rawCounter = null;
        checkpointRevision = 0; NextSequence = null;
    }

    public void Accept(byte[] frame, int generation)
    {
        var header = Header(frame, generation);
        if (header.Type == 3)
        {
            if (checkpointCounter is { } previous && header.Counter <= previous || header.First <= checkpointRevision)
                throw ApiException.Invalid("Terminal checkpoint replay was rejected.");
            checkpointCounter = header.Counter; checkpointRevision = header.First;
            if (NextSequence is { } live && header.Next < live)
                throw ApiException.Invalid("A broadcast checkpoint cannot rewind the live terminal.");
            NextSequence = header.Next;
            raw.Clear(); rawBytes = 0;
            return;
        }
        if (rawCounter is { } counter && header.Counter <= counter) throw ApiException.Invalid("Terminal output replay was rejected.");
        if (header.First >= header.Next || NextSequence is { } expected && header.First != expected)
            throw ApiException.Invalid("Terminal output sequence is not contiguous.");
        rawCounter = header.Counter; NextSequence = header.Next;
        raw.Enqueue(new RawFrame(header.First, header.Next, frame)); rawBytes += frame.Length;
        while (raw.Count > 256 || rawBytes > MaximumRawBytes)
            rawBytes -= raw.Dequeue().Bytes.Length;
    }

    public IReadOnlyList<byte[]>? Bootstrap(byte[] checkpoint, int generation)
    {
        var header = Header(checkpoint, generation);
        if (header.Type != 3) throw ApiException.Invalid("A checkpoint response must contain a checkpoint frame.");
        var cursor = header.Next;
        var result = new List<byte[]> { checkpoint };
        if (NextSequence is null || cursor > NextSequence) return result;
        foreach (var frame in raw)
        {
            if (frame.Next <= cursor) continue;
            if (frame.First != cursor) return null;
            result.Add(frame.Bytes); cursor = frame.Next;
        }
        return cursor == NextSequence ? result : null;
    }

    public static FrameHeader Header(byte[] frame, int generation)
    {
        if (frame.Length < 45 || frame.Length > MaximumFrame || frame[0] is not (3 or 4))
            throw ApiException.Invalid("Unsupported terminal frame.");
        var span = frame.AsSpan();
        if (BinaryPrimitives.ReadUInt32BigEndian(span[1..5]) != generation)
            throw ApiException.Conflict("Terminal key generation changed.");
        return new FrameHeader(frame[0], BinaryPrimitives.ReadUInt64BigEndian(span[5..13]),
            BinaryPrimitives.ReadUInt64BigEndian(span[13..21]), BinaryPrimitives.ReadUInt64BigEndian(span[21..29]));
    }
    private sealed record RawFrame(ulong First, ulong Next, byte[] Bytes);
    public readonly record struct FrameHeader(byte Type, ulong Counter, ulong First, ulong Next);
}
