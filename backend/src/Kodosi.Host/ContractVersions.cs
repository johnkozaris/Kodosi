namespace Kodosi;

internal static class ContractVersions
{
    public const int Api = 32;
    public const int OldestApi = 32;
    public const int Auth = 1;
    public const int TerminalConnection = 24;
    public const int OldestTerminalConnection = 24;

    public static object Report() => new
    {
        status = "ok",
        apiContractVersion = Api,
        oldestApiContractVersion = OldestApi,
        authContractVersion = Auth,
        terminalConnectionProtocolVersion = TerminalConnection,
        oldestTerminalConnectionProtocolVersion = OldestTerminalConnection,
    };
}
