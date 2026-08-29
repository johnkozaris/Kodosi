using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class EnforceRoomMemberAdmissionProof : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddCheckConstraint(
                name: "CK_room_members_active_admission_proof",
                table: "room_members",
                sql: "\"revoked_at\" IS NOT NULL\nOR \"role\" = 'Owner'\nOR (\n    \"admission_invitation_id\" IS NOT NULL\n    AND \"admission_proposal_body\" IS NOT NULL\n    AND \"admission_proposal_signature\" IS NOT NULL\n    AND \"admission_proposal_signer_device_id\" IS NOT NULL\n    AND \"admission_proposal_hash\" IS NOT NULL\n    AND \"admission_decision_body\" IS NOT NULL\n    AND \"admission_decision_signature\" IS NOT NULL\n    AND \"admission_decision_signer_device_id\" IS NOT NULL\n    AND \"admission_expires_at\" IS NOT NULL\n)");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_room_members_active_admission_proof",
                table: "room_members");
        }
    }
}
