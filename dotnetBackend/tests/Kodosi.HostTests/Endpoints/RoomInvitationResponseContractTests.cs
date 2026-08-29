using System.Text.Json;
using Kodosi.Application;
using Kodosi.Domain;
using Kodosi.Host.Endpoints;
using Kodosi.Host.Serialization;

namespace Kodosi.HostTests;

public sealed class RoomInvitationResponseContractTests
{
    [Fact]
    public void Action_Response_Matches_Rust_RoomInvitationDto()
    {
        var ownerId = UserId.New();
        var inviteeId = UserId.New();
        var now = DateTimeOffset.UtcNow;
        var invitation = RoomInvitation.Create(
            Guid.NewGuid(),
            RoomId.From(Guid.NewGuid()),
            inviteeId,
            ownerId,
            new RoomInvitationProposalProof(
                1,
                2,
                [1],
                [2],
                "owner-device",
                [3],
                [4],
                "owner-device",
                new byte[32],
                now,
                now.AddDays(1)),
            now);
        var response = RoomResponseMappers.MapInvitation(
            new RoomInvitationWithContext(
                invitation,
                "Mission Control",
                "mission-control",
                "owner",
                "Owner",
                "invitee",
                null));
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);

        using var document = JsonDocument.Parse(JsonSerializer.Serialize(response, options));
        var root = document.RootElement;

        Assert.Equal(
            [
                "id",
                "roomId",
                "roomName",
                "roomSlug",
                "inviteeUserId",
                "inviteeHandle",
                "inviteeDisplayName",
                "invitedByUserId",
                "invitedByHandle",
                "invitedByDisplayName",
                "status",
                "createdAt",
                "respondedAt",
                "baseRosterGeneration",
                "proposedRosterGeneration",
                "proposedRosterBody",
                "proposedRosterSignature",
                "proposedRosterSignerDeviceId",
                "proposalBody",
                "proposalSignature",
                "proposalSignerDeviceId",
                "proposalHash",
                "proposalIssuedAt",
                "expiresAt",
                "decisionBody",
                "decisionSignature",
                "decisionSignerDeviceId",
                "decisionIssuedAt",
            ],
            root.EnumerateObject().Select(static property => property.Name));
        Assert.Equal(invitation.Id, root.GetProperty("id").GetGuid());
        Assert.Equal(invitation.RoomId.Value, root.GetProperty("roomId").GetGuid());
        Assert.Equal(inviteeId.Value, root.GetProperty("inviteeUserId").GetGuid());
        Assert.Equal(ownerId.Value, root.GetProperty("invitedByUserId").GetGuid());
        Assert.Equal("Pending", root.GetProperty("status").GetString());
        Assert.Equal(JsonValueKind.Null, root.GetProperty("inviteeDisplayName").ValueKind);
    }

    [Fact]
    public void Room_Response_Carries_Composite_Activation_Proof()
    {
        var now = DateTimeOffset.UtcNow;
        var ownerId = UserId.New();
        var inviteeId = UserId.New();
        var room = Room.Create(
            RoomId.From(Guid.NewGuid()),
            ownerId,
            "Mission Control",
            "mission-control",
            1,
            [1],
            [2],
            "owner-device");
        var invitation = RoomInvitation.Create(
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
        invitation.Accept(
            inviteeId,
            new RoomInvitationDecisionProof(
                [7],
                [8],
                "invitee-device",
                now.AddMinutes(1)),
            now.AddMinutes(1));
        room.ActivateInvitation(invitation);
        var member = RoomMember.CreateFromInvitation(invitation);
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);

        using var document = JsonDocument.Parse(JsonSerializer.Serialize(
            RoomResponseMappers.MapRoom(room, [member]),
            options));
        var proof = document.RootElement.GetProperty("rosterActivationProof");
        var admissionProof = Assert.Single(
            document.RootElement.GetProperty("admissionProofs").EnumerateArray());

        Assert.Equal(invitation.Id, proof.GetProperty("invitationId").GetGuid());
        Assert.Equal(inviteeId.Value, proof.GetProperty("inviteeUserId").GetGuid());
        Assert.Equal(
            Convert.ToBase64String(invitation.ProposalBody),
            proof.GetProperty("proposalBody").GetString());
        Assert.Equal(
            Convert.ToBase64String(invitation.DecisionBody!),
            proof.GetProperty("decisionBody").GetString());
        Assert.Equal(
            inviteeId.Value,
            admissionProof.GetProperty("inviteeUserId").GetGuid());
        Assert.Equal(
            Convert.ToBase64String(invitation.ProposalBody),
            admissionProof.GetProperty("proposalBody").GetString());
        Assert.Equal(
            Convert.ToBase64String(invitation.DecisionSignature!),
            admissionProof.GetProperty("decisionSignature").GetString());
    }

    [Fact]
    public void Invitation_Request_Shapes_Match_Rust_Backend_Client()
    {
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web);
        HostJsonSerializerOptions.Configure(options);
        using var proposalDocument = JsonDocument.Parse(JsonSerializer.Serialize(
            new InviteRoomMemberRequest(
                Guid.NewGuid(),
                Guid.NewGuid(),
                "proposal-body",
                "proposal-signature",
                "owner-device",
                2,
                "roster-body",
                "roster-signature",
                "owner-device"),
            options));
        Assert.Equal(
            [
                "invitationId",
                "inviteeUserId",
                "proposalBody",
                "proposalSignature",
                "proposalSignerDeviceId",
                "proposedRosterGeneration",
                "proposedRosterBody",
                "proposedRosterSignature",
                "proposedRosterSignerDeviceId",
            ],
            proposalDocument.RootElement
                .EnumerateObject()
                .Select(static property => property.Name));

        using var decisionDocument = JsonDocument.Parse(JsonSerializer.Serialize(
            new RoomInvitationDecisionRequest(
                Guid.CreateVersion7(),
                "decision-body",
                "decision-signature",
                "invitee-device"),
            options));
        Assert.Equal(
            [
                "requestId",
                "decisionBody",
                "decisionSignature",
                "decisionSignerDeviceId",
            ],
            decisionDocument.RootElement
                .EnumerateObject()
                .Select(static property => property.Name));
    }

    [Fact]
    public void Room_Response_Excludes_Revoked_Member_Admission_Proofs()
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
        var invitation = RoomInvitation.Create(
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
        invitation.Accept(
            inviteeId,
            new RoomInvitationDecisionProof([7], [8], "invitee-device", now),
            now);
        var member = RoomMember.CreateFromInvitation(invitation);
        member.Revoke();

        var response = RoomResponseMappers.MapRoom(room, [member]);

        Assert.Empty(response.AdmissionProofs);
    }
}
