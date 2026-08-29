using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class RoomInvitationAuthorizationTests
{
    [Fact]
    public async Task ReAdmission_Starts_A_New_Membership_Interval()
    {
        var now = DateTimeOffset.UtcNow;
        var ownerId = UserId.New();
        var inviteeId = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()),
            ownerId,
            "Room",
            "room",
            1,
            [1],
            [2],
            "owner-device");
        var member = DomainFixtureHydrator.RoomMember(room.Id, inviteeId, RoomRole.Member);
        var firstAdmission = member.CreatedAt;
        member.Revoke();
        var invitation = CreateInvitation(room, ownerId, inviteeId, now);
        invitation.Accept(
            inviteeId,
            new RoomInvitationDecisionProof([9], [8], "invitee-device", now.AddMinutes(1)),
            now.AddMinutes(1));
        await Task.Delay(1, TestContext.Current.CancellationToken);

        member.ApplyAcceptedInvitation(invitation);

        Assert.True(member.CreatedAt > firstAdmission);
        Assert.Null(member.RevokedAt);
    }

    [Fact]
    public void Accepted_Invitation_Activates_Exactly_Its_Proposed_Roster()
    {
        var now = DateTimeOffset.UtcNow;
        var ownerId = UserId.New();
        var inviteeId = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()),
            ownerId,
            "Room",
            "room",
            1,
            [1],
            [2],
            "owner-device");
        var invitation = CreateInvitation(room, ownerId, inviteeId, now);
        invitation.Accept(
            inviteeId,
            new RoomInvitationDecisionProof(
                [9],
                [8],
                "invitee-device",
                now.AddMinutes(1)),
            now.AddMinutes(1));

        room.ActivateInvitation(invitation);

        Assert.Equal(2, room.RosterGeneration);
        Assert.Equal([3], room.RosterBody);
        Assert.Equal(invitation.Id, room.RosterActivationInvitationId);

        room.ReplaceRosterForRemoval(3, [10], [11], "owner-device");
        Assert.Null(room.RosterActivationInvitationId);
        Assert.Null(room.RosterActivationDecisionBody);
    }

    [Fact]
    public void Decline_And_Cancel_Cannot_Activate_A_Roster()
    {
        var now = DateTimeOffset.UtcNow;
        var ownerId = UserId.New();
        var inviteeId = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()),
            ownerId,
            "Room",
            "room",
            1,
            [1],
            [2],
            "owner-device");
        var declined = CreateInvitation(room, ownerId, inviteeId, now);
        declined.Decline(
            inviteeId,
            new RoomInvitationDecisionProof(
                [9],
                [8],
                "invitee-device",
                now.AddMinutes(1)),
            now.AddMinutes(1));
        Assert.Throws<DomainException>(() => room.ActivateInvitation(declined));

        var cancelled = CreateInvitation(room, ownerId, inviteeId, now);
        cancelled.CancelByInviter(ownerId, now.AddMinutes(1));
        Assert.Throws<DomainException>(() => room.ActivateInvitation(cancelled));
        Assert.Equal(1, room.RosterGeneration);
    }

    private static RoomInvitation CreateInvitation(
        Room room,
        UserId ownerId,
        UserId inviteeId,
        DateTimeOffset now)
        => RoomInvitation.Create(
            Guid.NewGuid(),
            room.Id,
            inviteeId,
            ownerId,
            new RoomInvitationProposalProof(
                1,
                2,
                [3],
                [4],
                "owner-device",
                [5],
                [6],
                "owner-device",
                new byte[32],
                now,
                now.AddDays(1)),
            now);
}
