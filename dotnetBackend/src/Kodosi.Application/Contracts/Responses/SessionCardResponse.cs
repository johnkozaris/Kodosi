using Kodosi.Domain;

namespace Kodosi.Application;


public sealed record SessionCardResponse(
    string Id,
    string Title,
    SessionScope Scope,
    AccessLevel Access,
    SessionStatus Status,
    string OwnerUserId,
    string OwnerDisplayName,
    string? OwnerAvatarUrl,
    int ParticipantCount,
    DateTimeOffset StartedAt,
    ToolKind ToolKind,
    Guid? RoomId);
