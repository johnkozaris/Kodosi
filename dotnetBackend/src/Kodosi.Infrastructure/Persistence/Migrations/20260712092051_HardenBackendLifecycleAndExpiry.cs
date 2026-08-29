using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class HardenBackendLifecycleAndExpiry : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_room_invitations_status_allowed_values",
                table: "room_invitations");

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "expires_at",
                table: "session_access_overrides",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "admission_decision_body",
                table: "room_members",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "admission_decision_signature",
                table: "room_members",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "admission_decision_signer_device_id",
                table: "room_members",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "admission_expires_at",
                table: "room_members",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<Guid>(
                name: "admission_invitation_id",
                table: "room_members",
                type: "uuid",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "admission_proposal_body",
                table: "room_members",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "admission_proposal_hash",
                table: "room_members",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "admission_proposal_signature",
                table: "room_members",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "admission_proposal_signer_device_id",
                table: "room_members",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "acknowledged_at",
                table: "device_link_requests",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "result_expires_at",
                table: "device_link_requests",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.Sql(
                """
                UPDATE device_link_requests
                SET acknowledged_at = consumed_at,
                    result_expires_at = approved_at + INTERVAL '24 hours'
                WHERE approved_at IS NOT NULL;
                """);

            migrationBuilder.Sql(
                """
                UPDATE room_members AS member
                SET admission_invitation_id = room.roster_activation_invitation_id,
                    admission_proposal_body = room.roster_activation_proposal_body,
                    admission_proposal_signature = room.roster_activation_proposal_signature,
                    admission_proposal_signer_device_id = room.roster_activation_proposal_signer_device_id,
                    admission_proposal_hash = room.roster_activation_proposal_hash,
                    admission_decision_body = room.roster_activation_decision_body,
                    admission_decision_signature = room.roster_activation_decision_signature,
                    admission_decision_signer_device_id = room.roster_activation_decision_signer_device_id,
                    admission_expires_at = room.roster_activation_expires_at
                FROM rooms AS room
                WHERE member.room_id = room.id
                  AND member.user_id = room.roster_activation_invitee_user_id
                  AND member.revoked_at IS NULL
                  AND member.role <> 'Owner';
                """);

            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM room_members
                        WHERE revoked_at IS NULL
                          AND role <> 'Owner'
                          AND (
                              admission_invitation_id IS NULL
                              OR admission_proposal_body IS NULL
                              OR admission_proposal_signature IS NULL
                              OR admission_proposal_signer_device_id IS NULL
                              OR admission_proposal_hash IS NULL
                              OR admission_decision_body IS NULL
                              OR admission_decision_signature IS NULL
                              OR admission_decision_signer_device_id IS NULL
                              OR admission_expires_at IS NULL
                          )
                        LIMIT 1
                    ) THEN
                        RAISE EXCEPTION
                            'Active non-owner room members require durable admission proofs'
                            USING HINT = 'Export the room graph and re-admit affected members with signed invitation proposal and decision proofs.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.CreateIndex(
                name: "IX_session_access_overrides_expires_at",
                table: "session_access_overrides",
                column: "expires_at",
                filter: "\"revoked_at\" IS NULL AND \"expires_at\" IS NOT NULL");

            migrationBuilder.CreateIndex(
                name: "IX_room_members_admission_invitation_id",
                table: "room_members",
                column: "admission_invitation_id",
                unique: true,
                filter: "\"admission_invitation_id\" IS NOT NULL");

            migrationBuilder.AddCheckConstraint(
                name: "CK_room_invitations_status_allowed_values",
                table: "room_invitations",
                sql: "\"status\" IN ('Pending', 'Accepted', 'Declined', 'Cancelled', 'Expired', 'Superseded')");

            migrationBuilder.CreateIndex(
                name: "IX_device_link_requests_result_expires_at",
                table: "device_link_requests",
                column: "result_expires_at");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_session_access_overrides_expires_at",
                table: "session_access_overrides");

            migrationBuilder.DropIndex(
                name: "IX_room_members_admission_invitation_id",
                table: "room_members");

            migrationBuilder.DropCheckConstraint(
                name: "CK_room_invitations_status_allowed_values",
                table: "room_invitations");

            migrationBuilder.DropIndex(
                name: "IX_device_link_requests_result_expires_at",
                table: "device_link_requests");

            migrationBuilder.Sql(
                """
                UPDATE room_invitations
                SET status = 'Cancelled',
                    responded_at = COALESCE(responded_at, CURRENT_TIMESTAMP)
                WHERE status IN ('Expired', 'Superseded');
                """);

            migrationBuilder.DropColumn(
                name: "expires_at",
                table: "session_access_overrides");

            migrationBuilder.DropColumn(
                name: "admission_decision_body",
                table: "room_members");

            migrationBuilder.DropColumn(
                name: "admission_decision_signature",
                table: "room_members");

            migrationBuilder.DropColumn(
                name: "admission_decision_signer_device_id",
                table: "room_members");

            migrationBuilder.DropColumn(
                name: "admission_expires_at",
                table: "room_members");

            migrationBuilder.DropColumn(
                name: "admission_invitation_id",
                table: "room_members");

            migrationBuilder.DropColumn(
                name: "admission_proposal_body",
                table: "room_members");

            migrationBuilder.DropColumn(
                name: "admission_proposal_hash",
                table: "room_members");

            migrationBuilder.DropColumn(
                name: "admission_proposal_signature",
                table: "room_members");

            migrationBuilder.DropColumn(
                name: "admission_proposal_signer_device_id",
                table: "room_members");

            migrationBuilder.DropColumn(
                name: "acknowledged_at",
                table: "device_link_requests");

            migrationBuilder.DropColumn(
                name: "result_expires_at",
                table: "device_link_requests");

            migrationBuilder.AddCheckConstraint(
                name: "CK_room_invitations_status_allowed_values",
                table: "room_invitations",
                sql: "\"status\" IN ('Pending', 'Accepted', 'Declined', 'Cancelled')");
        }
    }
}
