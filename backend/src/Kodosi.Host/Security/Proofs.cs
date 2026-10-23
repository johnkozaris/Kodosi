using System.Buffers.Binary;
using System.Text;

namespace Kodosi.Security;

internal static class DomainTags
{
    public static ReadOnlySpan<byte> DevicePopV1 => "kodosi-device-pop-v1"u8;
    public static ReadOnlySpan<byte> DeviceSessionV1 => "kodosi-device-session-v1"u8;
    public static ReadOnlySpan<byte> DeviceCertV3 => "kodosi-device-cert-v3"u8;
    public static ReadOnlySpan<byte> DeviceListV1 => "kodosi-device-list-v1"u8;
    public static ReadOnlySpan<byte> RoomStateV1 => "kodosi-room-state-v1"u8;
    public static ReadOnlySpan<byte> RoomContentV1 => "kodosi-room-content-v1"u8;
    public static ReadOnlySpan<byte> RoomRecipientV1 => "kodosi-room-recipient-v1"u8;
    public static ReadOnlySpan<byte> TerminalRoomV1 => "kodosi-terminal-room-v1"u8;
    public static ReadOnlySpan<byte> TerminalCloseV1 => "kodosi-terminal-close-v1"u8;
}

internal static class Proofs
{
    public static byte[] Tagged(ReadOnlySpan<byte> domain, ReadOnlySpan<byte> body)
    {
        var result = new byte[domain.Length + body.Length];
        domain.CopyTo(result);
        body.CopyTo(result.AsSpan(domain.Length));
        return result;
    }

    public static byte[] DeviceSession(Guid user, string device, Guid challengeId, ReadOnlySpan<byte> challenge)
    {
        using var stream = new MemoryStream();
        stream.Write(DomainTags.DeviceSessionV1);
        Write(stream, user.ToString("D"));
        Write(stream, device);
        Write(stream, challengeId.ToString("D"));
        stream.Write(challenge);
        return stream.ToArray();
    }

    public static byte[] RoomRecipient(Guid user, string device, byte[] key)
    {
        using var stream = new MemoryStream();
        stream.Write(DomainTags.RoomRecipientV1);
        Write(stream, user.ToString("D"));
        Write(stream, device);
        Write(stream, key);
        return stream.ToArray();
    }

    public static byte[] TerminalRoom(Guid owner, Guid terminal, Guid incarnation, Guid? room, long sequence, string device)
    {
        using var stream = new MemoryStream();
        stream.Write(DomainTags.TerminalRoomV1);
        Write(stream, owner.ToString("D"));
        Write(stream, terminal.ToString("D"));
        Write(stream, incarnation.ToString("D"));
        Write(stream, room?.ToString("D") ?? "");
        Span<byte> number = stackalloc byte[sizeof(long)];
        BinaryPrimitives.WriteInt64BigEndian(number, sequence);
        Write(stream, number);
        Write(stream, device);
        return stream.ToArray();
    }

    public static byte[] TerminalClose(Guid owner, Guid terminal, Guid incarnation, Guid user, string device)
    {
        using var stream = new MemoryStream();
        stream.Write(DomainTags.TerminalCloseV1);
        Write(stream, owner.ToString("D"));
        Write(stream, terminal.ToString("D"));
        Write(stream, incarnation.ToString("D"));
        Write(stream, user.ToString("D"));
        Write(stream, device);
        return stream.ToArray();
    }

    private static void Write(Stream stream, string value) => Write(stream, Encoding.UTF8.GetBytes(value));

    private static void Write(Stream stream, ReadOnlySpan<byte> value)
    {
        Span<byte> length = stackalloc byte[sizeof(uint)];
        BinaryPrimitives.WriteUInt32BigEndian(length, checked((uint)value.Length));
        stream.Write(length);
        stream.Write(value);
    }
}
