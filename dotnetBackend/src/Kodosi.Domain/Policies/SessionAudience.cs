
namespace Kodosi.Domain;

public readonly record struct SessionAudience(SessionScope Scope, RoomId? RoomId)
{
    public static SessionAudience Of(SessionScope scope, RoomId? roomId) =>
        new(scope, scope == SessionScope.Room ? roomId : null);

    public static SessionAudience Of(Session session) =>
        Of(session.Scope, session.RoomId);

    public bool IsRoomMoveFrom(SessionAudience previous) =>
    Scope == SessionScope.Room
    && previous.Scope == SessionScope.Room
    && previous.RoomId != RoomId;
}
