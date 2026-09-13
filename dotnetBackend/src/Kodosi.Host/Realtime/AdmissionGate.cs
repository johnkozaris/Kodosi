namespace Kodosi.Realtime;

internal sealed class AdmissionFilter(AdmissionGate gate, RelayDirectory relay) : IEndpointFilter
{
    public async ValueTask<object?> InvokeAsync(EndpointFilterInvocationContext context, EndpointFilterDelegate next)
    {
        using var held = await gate.EnterAsync(context.HttpContext.RequestAborted);
        using var recovery = relay.BeginMutation();
        try { return await next(context); }
        catch { recovery.Recover(); throw; }
    }
}

public sealed class AdmissionGate
{
    private readonly SemaphoreSlim gate = new(1, 1);

    public async ValueTask<IDisposable> EnterAsync(CancellationToken cancellationToken)
    {
        await gate.WaitAsync(cancellationToken);
        return new Lease(gate);
    }

    private sealed class Lease(SemaphoreSlim gate) : IDisposable
    {
        private SemaphoreSlim? held = gate;
        public void Dispose() => Interlocked.Exchange(ref held, null)?.Release();
    }
}
