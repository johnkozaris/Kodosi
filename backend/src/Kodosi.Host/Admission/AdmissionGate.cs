using Kodosi.TerminalConnections;

namespace Kodosi.Admission;

internal sealed class AdmissionFilter(AdmissionGate gate, ConnectionDirectory connections) : IEndpointFilter
{
    public async ValueTask<object?> InvokeAsync(EndpointFilterInvocationContext context, EndpointFilterDelegate next)
    {
        var request = context.HttpContext;
        if (HttpMethods.IsGet(request.Request.Method))
        {
            using var shared = await gate.EnterSharedAsync(request.RequestAborted);
            return await next(context);
        }
        using var held = await gate.EnterAsync(request.RequestAborted);
        using var recovery = connections.BeginMutation();
        try { return await next(context); }
        catch { recovery.Recover(); throw; }
    }
}

public sealed class AdmissionGate
{
    private const int MaximumReaders = 32;
    private readonly SemaphoreSlim turnstile = new(1, 1);
    private readonly SemaphoreSlim empty = new(1, 1);
    private readonly SemaphoreSlim slots = new(MaximumReaders, MaximumReaders);
    private readonly object sync = new();
    private int readers;

    public async ValueTask<IDisposable> EnterAsync(CancellationToken cancellationToken)
    {
        await turnstile.WaitAsync(cancellationToken);
        try { await empty.WaitAsync(cancellationToken); }
        catch { turnstile.Release(); throw; }
        return new Lease(this, shared: false);
    }

    public async ValueTask<IDisposable> EnterSharedAsync(CancellationToken cancellationToken)
    {
        await slots.WaitAsync(cancellationToken);
        try { await turnstile.WaitAsync(cancellationToken); }
        catch { slots.Release(); throw; }
        lock (sync) if (readers++ == 0) empty.Wait(CancellationToken.None);
        turnstile.Release();
        return new Lease(this, shared: true);
    }

    private void Exit(bool shared)
    {
        if (!shared) { empty.Release(); turnstile.Release(); return; }
        lock (sync) if (--readers == 0) empty.Release();
        slots.Release();
    }

    private sealed class Lease(AdmissionGate gate, bool shared) : IDisposable
    {
        private AdmissionGate? held = gate;
        public void Dispose() => Interlocked.Exchange(ref held, null)?.Exit(shared);
    }
}
