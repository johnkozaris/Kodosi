namespace Kodosi;

internal static class ContractVersions
{
    public const int Api = 27;
    public const int OldestApi = 27;
    public const int Auth = 1;
    public const int TerminalConnection = 22;
    public const int OldestTerminalConnection = 22;

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
