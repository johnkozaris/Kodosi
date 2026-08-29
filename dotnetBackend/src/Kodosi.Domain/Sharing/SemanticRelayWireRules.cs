namespace Kodosi.Domain;

public static class SemanticRelayWireRules
{
    public const string QueueMode = "queue";
    public const string SteerMode = "steer";
    public const string StopAndSendMode = "stopAndSend";
    public const string InjectedOutcome = "injected";
    public const string CancelledOutcome = "cancelled";
    public const string DeliveryUnknownOutcome = "deliveryUnknown";

    public static bool IsMode(string? value) =>
        value is QueueMode or SteerMode or StopAndSendMode;

    public static bool IsOutcome(string? value) =>
        value is InjectedOutcome or CancelledOutcome or DeliveryUnknownOutcome;

    public static bool IsLowerHexSha256(string? value) =>
        value is { Length: 64 }
        && value.All(static character =>
            character is >= '0' and <= '9' or >= 'a' and <= 'f');

    public static bool IsCanonicalUuidV7(Guid value)
    {
        Span<byte> bytes = stackalloc byte[16];
        return value.TryWriteBytes(bytes, bigEndian: true, out var written)
            && written == bytes.Length
            && (bytes[6] >> 4) == 7
            && (bytes[8] & 0xC0) == 0x80;
    }
}
