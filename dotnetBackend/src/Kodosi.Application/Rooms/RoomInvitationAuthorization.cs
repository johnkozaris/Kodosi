using Kodosi.Domain;

namespace Kodosi.Application;

internal static class RoomInvitationAuthorization
{
    public static void EnsureActor(
        RoomInvitation invitation,
        UserId actingUserId,
        bool inviteeAction)
    {
        var authorized = inviteeAction
            ? invitation.InviteeUserId == actingUserId
            : invitation.InvitedByUserId == actingUserId;
        if (!authorized)
        {
            throw new NotFoundException("RoomInvitation", invitation.Id);
        }
    }
}
