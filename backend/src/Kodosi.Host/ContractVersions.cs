namespace Kodosi;

internal static class ContractVersions
{
    public const int Api = 29;
    public const int OldestApi = 29;
    public const int Auth = 1;
    public const int TerminalConnection = 23;
    public const int OldestTerminalConnection = 22;
    public const int Calls = 23;

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
