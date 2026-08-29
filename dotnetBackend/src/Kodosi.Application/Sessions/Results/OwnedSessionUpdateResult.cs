
namespace Kodosi.Application;

public sealed record OwnedSessionUpdateResult(
    SessionDetailResponse Response,
    SessionDiscoveryTarget Before,
    SessionDiscoveryTarget After,
    IAsyncDisposable? Lifecycle = null) : IAsyncDisposable
{
    public ValueTask DisposeAsync() =>
        Lifecycle?.DisposeAsync() ?? ValueTask.CompletedTask;
}
