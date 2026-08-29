using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddStagedRoomInvitationAuthorization : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_room_invitations_status_allowed_values",
                table: "room_invitations");

            migrationBuilder.AddColumn<byte[]>(
                name: "roster_activation_decision_body",
                table: "rooms",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "roster_activation_decision_signature",
                table: "rooms",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "roster_activation_decision_signer_device_id",
                table: "rooms",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "roster_activation_expires_at",
                table: "rooms",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<Guid>(
                name: "roster_activation_invitation_id",
                table: "rooms",
                type: "uuid",
                nullable: true);

            migrationBuilder.AddColumn<Guid>(
                name: "roster_activation_invitee_user_id",
                table: "rooms",
                type: "uuid",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "roster_activation_proposal_body",
                table: "rooms",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "roster_activation_proposal_hash",
                table: "rooms",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "roster_activation_proposal_signature",
                table: "rooms",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "roster_activation_proposal_signer_device_id",
                table: "rooms",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<uint>(
                name: "xmin",
                table: "rooms",
                type: "xid",
                rowVersion: true,
                nullable: false,
                defaultValue: 0u);

            migrationBuilder.AddColumn<long>(
                name: "base_roster_generation",
                table: "room_invitations",
                type: "bigint",
                nullable: false,
                defaultValue: 0L);

            migrationBuilder.AddColumn<byte[]>(
                name: "decision_body",
                table: "room_invitations",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "decision_issued_at",
                table: "room_invitations",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "decision_signature",
                table: "room_invitations",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "decision_signer_device_id",
                table: "room_invitations",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "expires_at",
                table: "room_invitations",
                type: "timestamp with time zone",
                nullable: false,
                defaultValue: new DateTimeOffset(new DateTime(1, 1, 1, 0, 0, 0, 0, DateTimeKind.Unspecified), new TimeSpan(0, 0, 0, 0, 0)));

            migrationBuilder.AddColumn<byte[]>(
                name: "proposal_body",
                table: "room_invitations",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);

            migrationBuilder.AddColumn<byte[]>(
                name: "proposal_hash",
                table: "room_invitations",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "proposal_issued_at",
                table: "room_invitations",
                type: "timestamp with time zone",
                nullable: false,
                defaultValue: new DateTimeOffset(new DateTime(1, 1, 1, 0, 0, 0, 0, DateTimeKind.Unspecified), new TimeSpan(0, 0, 0, 0, 0)));

            migrationBuilder.AddColumn<byte[]>(
                name: "proposal_signature",
                table: "room_invitations",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);

            migrationBuilder.AddColumn<string>(
                name: "proposal_signer_device_id",
                table: "room_invitations",
                type: "character varying(256)",
                maxLength: 256,
                nullable: false,
                defaultValue: "");

            migrationBuilder.AddColumn<byte[]>(
                name: "proposed_roster_body",
                table: "room_invitations",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);

            migrationBuilder.AddColumn<long>(
                name: "proposed_roster_generation",
                table: "room_invitations",
                type: "bigint",
                nullable: false,
                defaultValue: 0L);

            migrationBuilder.AddColumn<byte[]>(
                name: "proposed_roster_signature",
                table: "room_invitations",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);

            migrationBuilder.AddColumn<string>(
                name: "proposed_roster_signer_device_id",
                table: "room_invitations",
                type: "character varying(256)",
                maxLength: 256,
                nullable: false,
                defaultValue: "");

            migrationBuilder.CreateIndex(
                name: "IX_room_invitations_expires_at",
                table: "room_invitations",
                column: "expires_at");




            migrationBuilder.Sql(
                """
                UPDATE room_invitations
                SET status = 'Cancelled',
                    responded_at = COALESCE(responded_at, CURRENT_TIMESTAMP)
                WHERE status = 'Pending';
                """);

            migrationBuilder.AddCheckConstraint(
                name: "CK_room_invitations_status_allowed_values",
                table: "room_invitations",
                sql: "\"status\" IN ('Pending', 'Accepted', 'Declined', 'Cancelled')");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_room_invitations_expires_at",
                table: "room_invitations");

            migrationBuilder.DropCheckConstraint(
                name: "CK_room_invitations_status_allowed_values",
                table: "room_invitations");

            migrationBuilder.Sql(
                """
                UPDATE room_invitations
                SET status = 'Declined'
                WHERE status = 'Cancelled';
                """);

            migrationBuilder.DropColumn(
                name: "roster_activation_decision_body",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_activation_decision_signature",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_activation_decision_signer_device_id",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_activation_expires_at",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_activation_invitation_id",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_activation_invitee_user_id",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_activation_proposal_body",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_activation_proposal_hash",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_activation_proposal_signature",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_activation_proposal_signer_device_id",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "xmin",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "base_roster_generation",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "decision_body",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "decision_issued_at",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "decision_signature",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "decision_signer_device_id",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "expires_at",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "proposal_body",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "proposal_hash",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "proposal_issued_at",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "proposal_signature",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "proposal_signer_device_id",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "proposed_roster_body",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "proposed_roster_generation",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "proposed_roster_signature",
                table: "room_invitations");

            migrationBuilder.DropColumn(
                name: "proposed_roster_signer_device_id",
                table: "room_invitations");

            migrationBuilder.AddCheckConstraint(
                name: "CK_room_invitations_status_allowed_values",
                table: "room_invitations",
                sql: "\"status\" IN ('Pending', 'Accepted', 'Declined')");
        }
    }
}
