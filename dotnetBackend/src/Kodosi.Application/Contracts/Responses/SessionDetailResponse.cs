using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record SessionDetailResponse(
    Guid Id,
    Guid IncarnationId,
    long IncarnationGeneration,
    int IncarnationProtocolVersion,
    Guid OwnerUserId,
    string Title,
    ToolKind ToolKind,
    SessionScope Scope,
    Guid? RoomId,
    AccessLevel DefaultAccess,
    AccessLevel EffectiveAccess,
    SessionStatus Status,
    DateTimeOffset StartedAt,
    DateTimeOffset? EndedAt,
    DateTimeOffset LastHeartbeatAt);
