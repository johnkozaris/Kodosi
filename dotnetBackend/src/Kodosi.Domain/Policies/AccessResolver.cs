
namespace Kodosi.Domain;

public static class AccessResolver
{
    public static AccessLevel ResolveAccess(
        bool isOwner,
        SessionScope scope,
        AccessLevel defaultAccess,
        bool isFriend,
        bool isRoomMember,
        AccessLevel? explicitOverride)
    {
        return TryResolveAccess(
            isOwner,
            scope,
            defaultAccess,
            isFriend,
            isRoomMember,
            explicitOverride,
            out var accessLevel,
            out var denialMessage)
            ? accessLevel
            : throw new PolicyViolationException(denialMessage!);
    }

    public static bool TryResolveAccess(
        bool isOwner,
        SessionScope scope,
        AccessLevel defaultAccess,
        bool isFriend,
        bool isRoomMember,
        AccessLevel? explicitOverride,
        out AccessLevel accessLevel)
        => TryResolveAccess(
            isOwner,
            scope,
            defaultAccess,
            isFriend,
            isRoomMember,
            explicitOverride,
            out accessLevel,
            out _);

    private static bool TryResolveAccess(
        bool isOwner,
        SessionScope scope,
        AccessLevel defaultAccess,
        bool isFriend,
        bool isRoomMember,
        AccessLevel? explicitOverride,
        out AccessLevel accessLevel,
        out string? denialMessage)
    {
        if (isOwner)
        {
            accessLevel = AccessLevel.Inject;
            denialMessage = null;
            return true;
        }

        if (explicitOverride.HasValue)
        {
            accessLevel = explicitOverride.Value;
            denialMessage = null;
            return true;
        }

        switch (scope)
        {
            case SessionScope.JustMe:
                accessLevel = default;
                denialMessage = "Just-me sessions are not accessible to non-owners.";
                return false;
            case SessionScope.MyDevices:
                accessLevel = default;
                denialMessage = "My-devices sessions are restricted to the owner's other devices.";
                return false;
            case SessionScope.Friends when !isFriend:
                accessLevel = default;
                denialMessage = "Only friends may access this session.";
                return false;
            case SessionScope.Friends:
                accessLevel = defaultAccess;
                denialMessage = null;
                return true;
            case SessionScope.Room when !isRoomMember:
                accessLevel = default;
                denialMessage = "Only room members may access this session.";
                return false;
            case SessionScope.Room:
                accessLevel = defaultAccess;
                denialMessage = null;
                return true;
            default:
                throw new ArgumentOutOfRangeException(
                    nameof(scope),
                    scope,
                    "Unknown coding session scope.");
        }
    }
}
