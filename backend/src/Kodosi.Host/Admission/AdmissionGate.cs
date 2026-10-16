using Kodosi.Accounts;

namespace Kodosi.Admission;

internal sealed class AdmissionFilter(AccountGate gate) : IEndpointFilter
{
    public async ValueTask<object?> InvokeAsync(EndpointFilterInvocationContext context, EndpointFilterDelegate next)
    {
        using var turn = await gate.AdmitAsync(context.HttpContext);
        return await next(context);
    }
}

public sealed class AccountGate
{
    private readonly Lock sync = new();
    private readonly Dictionary<Guid, Turn> turns = [];

    public async ValueTask<IDisposable> EnterAsync(Guid account, CancellationToken cancellationToken) => await TakeAsync(account, cancellationToken);

    // The writes of one account run one at a time, so a device check made in the turn holds until the handler returns.
    internal async ValueTask<IDisposable?> AdmitAsync(HttpContext context)
    {
        if (HttpMethods.IsGet(context.Request.Method)) return null;
        if (context.Features.Get<Lease>() is { } held) return held;
        var user = await context.RequestServices.GetRequiredService<CurrentUser>().GetAsync(context, context.RequestAborted);
        var lease = await TakeAsync(user.Id, context.RequestAborted);
        context.Features.Set(lease);
        context.Response.RegisterForDispose(lease);
        return lease;
    }

    private async ValueTask<Lease> TakeAsync(Guid account, CancellationToken cancellationToken)
    {
        Turn? turn;
        lock (sync)
        {
            if (!turns.TryGetValue(account, out turn)) turns.Add(account, turn = new Turn());
            turn.Users++;
        }
        try { await turn.Semaphore.WaitAsync(cancellationToken); }
        catch { Leave(account, turn, held: false); throw; }
        return new Lease(this, account, turn);
    }

    private void Leave(Guid account, Turn turn, bool held)
    {
        if (held) turn.Semaphore.Release();
        lock (sync)
        {
            if (--turn.Users != 0) return;
            turns.Remove(account); turn.Semaphore.Dispose();
        }
    }

    private sealed class Turn
    {
        public SemaphoreSlim Semaphore { get; } = new(1, 1);
        public int Users;
    }

    private sealed class Lease(AccountGate gate, Guid account, Turn turn) : IDisposable
    {
        private AccountGate? held = gate;
        public void Dispose() => Interlocked.Exchange(ref held, null)?.Leave(account, turn, held: true);
    }
}
