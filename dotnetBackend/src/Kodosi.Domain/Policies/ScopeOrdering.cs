
namespace Kodosi.Domain;

public static class ScopeOrdering
{
    private static bool IsNarrowing(SessionScope oldScope, SessionScope newScope)
    {
        if (oldScope == newScope)
        {
            return false;
        }

        return (oldScope, newScope) switch
        {
            (SessionScope.Friends, SessionScope.JustMe) => true,
            (SessionScope.Friends, SessionScope.MyDevices) => true,
            (SessionScope.Room, SessionScope.JustMe) => true,
            (SessionScope.Room, SessionScope.MyDevices) => true,
            _ => false,
        };
    }

    public static bool RequiresOverrideReconciliation(
    SessionAudience oldAudience,
    SessionAudience newAudience)
    => newAudience.IsRoomMoveFrom(oldAudience)
        || RequiresOverrideReconciliation(oldAudience.Scope, newAudience.Scope);

    private static bool RequiresOverrideReconciliation(
        SessionScope oldScope,
        SessionScope newScope)
    {
        if (IsNarrowing(oldScope, newScope))
        {
            return true;
        }

        return (oldScope, newScope) switch
        {
            (SessionScope.Friends, SessionScope.Room) => true,
            (SessionScope.Room, SessionScope.Friends) => true,
            _ => false,
        };
    }
}
