using Kodosi.Domain;
using Kodosi.Infrastructure.Persistence;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.HostTests;

public sealed class RoomInvitationPersistenceModelTests
{
    [Fact]
    public void Invitation_And_Room_Models_Persist_Composite_Activation_Proof()
    {
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql("Host=localhost;Database=kodosi_model_test")
            .Options;
        using var context = new KodosiDbContext(options);

        var invitation = context.Model.FindEntityType(typeof(RoomInvitation))!;
        Assert.Equal(
            "proposal_body",
            invitation.FindProperty(nameof(RoomInvitation.ProposalBody))!.GetColumnName());
        Assert.Equal(
            "decision_signature",
            invitation.FindProperty(nameof(RoomInvitation.DecisionSignature))!.GetColumnName());
        Assert.Equal(
            "expires_at",
            invitation.FindProperty(nameof(RoomInvitation.ExpiresAt))!.GetColumnName());

        var room = context.Model.FindEntityType(typeof(Room))!;
        Assert.Equal(
            "roster_activation_proposal_hash",
            room.FindProperty(nameof(Room.RosterActivationProposalHash))!.GetColumnName());
        Assert.Equal(
            "roster_activation_decision_body",
            room.FindProperty(nameof(Room.RosterActivationDecisionBody))!.GetColumnName());
        Assert.True(room.FindProperty(nameof(Room.Version))!.IsConcurrencyToken);
    }
}
