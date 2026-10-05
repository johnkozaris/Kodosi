using System.Diagnostics.Metrics;

namespace Kodosi;

public sealed class ServerMetrics : IDisposable
{
    public const string MeterName = "Kodosi.Server";
    private readonly Meter meter = new(MeterName);
    private readonly Counter<long> relayBytes;
    private readonly Counter<long> refused;
    private readonly Counter<double> databaseFault;
    private Func<(int Devices, int Pipes)> connections = () => (0, 0);

    public ServerMetrics()
    {
        relayBytes = meter.CreateCounter<long>("kodosi.relay.bytes", "By", "Bytes copied between a host and a viewer.");
        refused = meter.CreateCounter<long>("kodosi.connections.refused", description: "Refused device connections and pipes by reason.");
        databaseFault = meter.CreateCounter<double>("kodosi.database.fault", "s", "Seconds in which the database was not reachable.");
        meter.CreateObservableGauge("kodosi.device.connections", () => connections().Devices, description: "Open device connections.");
        meter.CreateObservableGauge("kodosi.relay.pipes", () => connections().Pipes, description: "Open relay pipes.");
    }

    public void Observe(Func<(int Devices, int Pipes)> source) => connections = source;
    public void Relayed(int bytes) => relayBytes.Add(bytes);
    public void Refused(string reason) => refused.Add(1, new KeyValuePair<string, object?>("reason", reason));
    public void DatabaseFault(TimeSpan duration) => databaseFault.Add(duration.TotalSeconds);
    public void Dispose() => meter.Dispose();
}
