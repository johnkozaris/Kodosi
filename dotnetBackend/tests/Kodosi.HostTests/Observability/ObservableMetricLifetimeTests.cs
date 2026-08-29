using System.Diagnostics.Metrics;
using System.Reflection;
using Kodosi.Host.Observability;
using Kodosi.Infrastructure.Realtime;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging.Abstractions;

namespace Kodosi.HostTests;

public sealed class ObservableMetricLifetimeTests
{
    [Fact]
    public void OperationalMetrics_Dispose_Unregisters_Instance_Observations()
    {
        var metrics = new OperationalMetrics(new LiveSessionStateDirectory());
        var meter = GetObservableMeter(metrics);
        using var listener = CreateListener(
            meter,
            "Kodosi.realtime.active_sessions",
            out var instrument,
            out var measurements);
        listener.Start();
        listener.RecordObservableInstruments();
        Assert.Contains(instrument(), measurements);

        measurements.Clear();
        metrics.Dispose();
        listener.RecordObservableInstruments();

        Assert.DoesNotContain(instrument(), measurements);
    }

    [Fact]
    public void AuditWriter_Dispose_Unregisters_Queue_Depth_Observation()
    {
        using var services = new ServiceCollection().BuildServiceProvider();
        var writer = new AuditWriter(
            services.GetRequiredService<IServiceScopeFactory>(),
            NullLogger<AuditWriter>.Instance);
        var meter = GetObservableMeter(writer);
        using var listener = CreateListener(
            meter,
            "Kodosi.audit.queue_depth",
            out var instrument,
            out var measurements);
        listener.Start();
        listener.RecordObservableInstruments();
        Assert.Contains(instrument(), measurements);

        measurements.Clear();
        writer.Dispose();
        listener.RecordObservableInstruments();

        Assert.DoesNotContain(instrument(), measurements);
    }

    private static Meter GetObservableMeter(object owner) =>
        (Meter)owner.GetType()
            .GetField("_observableMeter", BindingFlags.Instance | BindingFlags.NonPublic)!
            .GetValue(owner)!;

    private static MeterListener CreateListener(
        Meter meter,
        string instrumentName,
        out Func<Instrument> instrument,
        out List<Instrument> measurements)
    {
        Instrument? publishedInstrument = null;
        measurements = [];
        var observedInstruments = measurements;
        var listener = new MeterListener();
        listener.InstrumentPublished = (candidate, activeListener) =>
        {
            if (ReferenceEquals(candidate.Meter, meter) && candidate.Name == instrumentName)
            {
                publishedInstrument = candidate;
                activeListener.EnableMeasurementEvents(candidate);
            }
        };
        listener.SetMeasurementEventCallback<int>(
            (candidate, _, _, _) => observedInstruments.Add(candidate));
        instrument = () => Assert.IsAssignableFrom<Instrument>(publishedInstrument);
        return listener;
    }
}
