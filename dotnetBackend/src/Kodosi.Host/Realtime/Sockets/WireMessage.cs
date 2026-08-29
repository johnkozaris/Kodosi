namespace Kodosi.Host.Realtime;



internal enum WireMessageKind : byte
{
    Json = 0,
    EncryptedBinary = 1,
}

internal readonly record struct WireMessage(WireMessageKind Kind, byte[] Payload)
{
    public static WireMessage Json(byte[] payload) => new(WireMessageKind.Json, payload);

    public static WireMessage EncryptedBinary(byte[] payload) => new(WireMessageKind.EncryptedBinary, payload);
}
