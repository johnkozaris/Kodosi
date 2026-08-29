namespace Kodosi.Domain;

public static class RelayProtocolVersions
{
    public const int Current = 10;

    public static bool IsAccepted(int? advertised) => advertised == Current;
}
