using Kodosi.Domain;

namespace Kodosi.Application;

public interface ISessionIncarnationRepository
{
    Task<SessionIncarnationRecord?> GetByIdempotencyKeyAsync(
        SessionId sessionId,
        Guid idempotencyKey,
        CancellationToken ct = default);

    Task AddAsync(
        SessionIncarnationRecord incarnation,
        CancellationToken ct = default);
}

public sealed record SessionIncarnationRecord(
    SessionId SessionId,
    long Generation,
    Guid IncarnationId,
    int ProtocolVersion,
    Guid IdempotencyKey,
    DateTimeOffset CreatedAt);
