using Kodosi.Admission;
using Xunit;

namespace Kodosi.HostTests;

public sealed class AdmissionGateTests
{
    [Fact]
    public async Task AnAbandonedChangeDoesNotBlockLaterChanges()
    {
        var ct = TestContext.Current.CancellationToken;
        var gate = new AccountGate();
        var account = Guid.CreateVersion7();
        var first = await gate.EnterAsync(account, ct);
        using var abandon = new CancellationTokenSource();
        var abandoned = gate.EnterAsync(account, abandon.Token).AsTask();
        await abandon.CancelAsync();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => abandoned);
        first.Dispose();
        (await gate.EnterAsync(account, ct).AsTask().WaitAsync(TimeSpan.FromSeconds(2), ct)).Dispose();
    }
}
