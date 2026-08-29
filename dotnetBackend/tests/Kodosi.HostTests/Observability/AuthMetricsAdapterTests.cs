using Kodosi.Application;
using Kodosi.Host.Observability;
using Kodosi.Infrastructure.Realtime;

namespace Kodosi.HostTests;

public sealed class AuthMetricsAdapterTests
{
    [Fact]
    public void Unmapped_Pop_Failure_Reason_Fails_Visibly()
    {
        var runtimes = new LiveSessionStateDirectory();
        var adapter = new AuthMetricsAdapter(new OperationalMetrics(runtimes));

        Assert.Throws<ArgumentOutOfRangeException>(() =>
            adapter.RecordPopFailure((PopFailureReason)int.MaxValue, "identity_reset"));
    }
}
