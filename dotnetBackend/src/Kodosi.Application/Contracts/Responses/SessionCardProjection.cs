using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record SessionCardProjection(
    Guid Id,
    string Title,
    SessionScope Scope,
    AccessLevel DefaultAccess,
    SessionStatus Status,
    Guid OwnerUserId,
    string OwnerDisplayName,
    string? OwnerAvatarUrl,
    DateTimeOffset StartedAt,
    Guid IncarnationId,
    ToolKind ToolKind,
    Guid? RoomId);
