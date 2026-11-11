namespace Kodosi;

internal static class ContractVersions
{
    public const int Api = 33;
    public const int OldestApi = 33;
    public const int Auth = 1;
    public const int TerminalConnection = 25;
    public const int OldestTerminalConnection = 25;

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
