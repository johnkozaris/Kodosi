using Kodosi.Admission;
using Xunit;

namespace Kodosi.HostTests;

public sealed class AdmissionGateTests
{
    [Fact]
    public async Task ReadsShareAdmissionAndAChangeWaitsForThemAndGoesBeforeLaterReads()
    {
        var ct = TestContext.Current.CancellationToken;
        var gate = new AdmissionGate();
        var first = await gate.EnterSharedAsync(ct);
        var second = await gate.EnterSharedAsync(ct);
        var change = gate.EnterAsync(ct).AsTask();
        var later = gate.EnterSharedAsync(ct).AsTask();
        first.Dispose();
        Assert.False(change.IsCompleted); Assert.False(later.IsCompleted);
        second.Dispose();
        var changing = await change.WaitAsync(TimeSpan.FromSeconds(2), ct);
        Assert.False(later.IsCompleted);
        changing.Dispose();
        (await later.WaitAsync(TimeSpan.FromSeconds(2), ct)).Dispose();
    }

    [Fact]
    public async Task AnAbandonedChangeDoesNotBlockLaterReadsOrChanges()
    {
        var ct = TestContext.Current.CancellationToken;
        var gate = new AdmissionGate();
        var reading = await gate.EnterSharedAsync(ct);
        using var abandon = new CancellationTokenSource();
        var abandoned = gate.EnterAsync(abandon.Token).AsTask();
        await abandon.CancelAsync();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => abandoned);
        (await gate.EnterSharedAsync(ct).AsTask().WaitAsync(TimeSpan.FromSeconds(2), ct)).Dispose();
        reading.Dispose();
        (await gate.EnterAsync(ct).AsTask().WaitAsync(TimeSpan.FromSeconds(2), ct)).Dispose();
    }
}
