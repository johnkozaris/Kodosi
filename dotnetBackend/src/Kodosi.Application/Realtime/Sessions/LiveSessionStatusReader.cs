using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class LiveSessionStatusReader(ISessionRepository sessions)
{
    private readonly ISessionRepository _sessions = sessions;

    public async Task<SessionStatus?> GetStatusAsync(
        SessionId sessionId,
        CancellationToken ct = default)
    {
        var session = await _sessions.GetByIdAsync(sessionId, ct);
        return session?.Status;
    }
}
