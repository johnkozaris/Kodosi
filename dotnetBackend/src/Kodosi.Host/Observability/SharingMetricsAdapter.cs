using Kodosi.Application;

namespace Kodosi.Host.Observability;

internal sealed class SharingMetricsAdapter(OperationalMetrics metrics) : ISharingMetrics
{
    private readonly OperationalMetrics _metrics = metrics;

    public void RecordSessionKeySkewRejection()
    {
        _metrics.RecordSessionKeySkewRejection();
    }
}
