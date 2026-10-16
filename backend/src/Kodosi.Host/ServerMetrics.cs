using System.Diagnostics.Metrics;
using Kodosi.Security;
using Kodosi.TerminalConnections;

namespace Kodosi;

public sealed class ServerMetrics
{
    public const string MeterName = "Kodosi.Server";
    private readonly Counter<long> refusedByReason;
    private long relayed;
    private long refused;
    private long limited;
    private long failed;

    public ServerMetrics(IMeterFactory meters, ConnectionDirectory connections, DeviceSessions sessions)
    {
        var meter = meters.Create(MeterName);
        refusedByReason = meter.CreateCounter<long>("kodosi.connections.refused", description: "Refused device connections and pipes by reason.");
        meter.CreateObservableCounter("kodosi.relay.bytes", () => Volatile.Read(ref relayed), "By", "Bytes copied between a host and a viewer.");
        meter.CreateObservableCounter("kodosi.requests.limited", () => Volatile.Read(ref limited), description: "Requests refused by a rate limit.");
        meter.CreateObservableCounter("kodosi.requests.failed", () => Volatile.Read(ref failed), description: "Requests that failed in the server.");
        meter.CreateObservableGauge("kodosi.device.connections", () => connections.Counts().Devices, description: "Open device connections.");
        meter.CreateObservableGauge("kodosi.relay.pipes", () => connections.Counts().Pipes, description: "Open relay pipes.");
        meter.CreateObservableGauge("kodosi.terminals.hosted", () => connections.Counts().Hosted, description: "Terminals whose host is connected.");
        meter.CreateObservableGauge("kodosi.relay.queued", () => connections.Relays.Used, "By", "Relay bytes that wait to be sent.");
        meter.CreateObservableGauge("kodosi.device.sessions", () => sessions.Count, description: "Open device sessions.");
    }

    public void Relayed(int bytes) => Interlocked.Add(ref relayed, bytes);

    public void Refused(string reason)
    {
        Interlocked.Increment(ref refused);
        refusedByReason.Add(1, new KeyValuePair<string, object?>("reason", reason));
    }

    public void Limited() => Interlocked.Increment(ref limited);

    public void Failed() => Interlocked.Increment(ref failed);

    internal Totals Read() => new(Volatile.Read(ref relayed), Volatile.Read(ref refused), Volatile.Read(ref limited), Volatile.Read(ref failed));

    internal readonly record struct Totals(long Relayed, long Refused, long Limited, long Failed);
}

internal sealed partial class ServerReport(ServerMetrics metrics, ConnectionDirectory connections, DeviceSessions sessions, TimeProvider clock,
    ILogger<ServerReport> logger) : PeriodicWork(TimeSpan.FromMinutes(1), clock, logger)
{
    private const double Megabyte = 1024 * 1024;
    private ServerMetrics.Totals last;

    protected internal override Task RunAsync(CancellationToken ct)
    {
        var now = metrics.Read();
        var (devices, pipes, hosted) = connections.Counts();
        Report(logger, devices, pipes, hosted, sessions.Count, (now.Relayed - last.Relayed) / Megabyte, now.Refused - last.Refused,
            now.Limited - last.Limited, now.Failed - last.Failed, connections.Relays.Used / Megabyte, GC.GetTotalMemory(false) / Megabyte,
            ThreadPool.PendingWorkItemCount);
        last = now;
        return Task.CompletedTask;
    }

    [LoggerMessage(Level = LogLevel.Information, Message = "Server: {Devices} device connections, {Pipes} pipes, {Hosted} hosted terminals, " +
        "{Sessions} device sessions; last minute: {RelayedMegabytes:F1} MB relayed, {Refused} refused, {Limited} rate limited, {Failed} failed; " +
        "{QueuedMegabytes:F1} MB queued, {HeapMegabytes:F0} MB heap, {PendingWork} pending work items")]
    private static partial void Report(ILogger logger, int devices, int pipes, int hosted, int sessions, double relayedMegabytes, long refused,
        long limited, long failed, double queuedMegabytes, double heapMegabytes, long pendingWork);
}
