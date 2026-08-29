using System.ComponentModel.DataAnnotations;
using Kodosi.Domain;

namespace Kodosi.Application;

public sealed record UpdateSessionRequest(
    [property: NotEmptyGuid]
    Guid ExpectedIncarnationId,
    [property: StringLength(256, MinimumLength = 1)]
    string? Title,
    [property: EnumDataType(typeof(SessionScope))]
    SessionScope? Scope,
    [property: EnumDataType(typeof(DefaultAudienceAccess))]
    DefaultAudienceAccess? DefaultAccess,
    [property: NotEmptyGuid]
    Guid? RoomId);
