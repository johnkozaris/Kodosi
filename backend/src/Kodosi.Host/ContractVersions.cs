namespace Kodosi;

internal static class ContractVersions
{
    public const int Api = 35;
    public const int OldestApi = 35;
    public const int Auth = 1;
    public const int TerminalConnection = 28;
    public const int OldestTerminalConnection = 28;

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
