using System.ComponentModel.DataAnnotations;
using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record CreateSessionRequest(
    [property: NotEmptyGuid]
    Guid Id,
    [property: UuidV7]
    Guid IdempotencyKey,
    [property: Required, StringLength(256, MinimumLength = 1)]
    string Title,
    [property: EnumDataType(typeof(SessionScope))]
    SessionScope Scope,
    [property: EnumDataType(typeof(ToolKind))]
    ToolKind ToolKind,
    [property: EnumDataType(typeof(DefaultAudienceAccess))]
    DefaultAudienceAccess DefaultAccess,
    [property: Required, StringLength(512, MinimumLength = 8)]
    string OwnerSecret,
    [property: NotEmptyGuid]
    Guid? RoomId);
