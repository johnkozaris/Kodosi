namespace Kodosi;

internal static class ContractVersions
{
    public const int Api = 35;
    public const int OldestApi = 35;
    public const int Auth = 1;
    public const int TerminalConnection = 27;
    public const int OldestTerminalConnection = 27;

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
