using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddPermissionDecisionPendingTuple : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<string>(
                name: "pending_request_id",
                table: "input_audit",
                type: "character varying(128)",
                maxLength: 128,
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "pending_requester_device_id",
                table: "input_audit",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<long>(
                name: "pending_session_incarnation_generation",
                table: "input_audit",
                type: "bigint",
                nullable: true);

            migrationBuilder.AddColumn<Guid>(
                name: "pending_session_incarnation_id",
                table: "input_audit",
                type: "uuid",
                nullable: true);

            migrationBuilder.AddCheckConstraint(
                name: "ck_input_audit_permission_pending_tuple",
                table: "input_audit",
                sql: "(pending_session_incarnation_id IS NULL AND pending_session_incarnation_generation IS NULL AND pending_request_id IS NULL AND pending_requester_device_id IS NULL) OR (pending_session_incarnation_id IS NOT NULL AND pending_session_incarnation_generation IS NOT NULL AND pending_session_incarnation_generation > 0 AND pending_request_id IS NOT NULL AND pending_requester_device_id IS NOT NULL)");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "ck_input_audit_permission_pending_tuple",
                table: "input_audit");

            migrationBuilder.DropColumn(
                name: "pending_request_id",
                table: "input_audit");

            migrationBuilder.DropColumn(
                name: "pending_requester_device_id",
                table: "input_audit");

            migrationBuilder.DropColumn(
                name: "pending_session_incarnation_generation",
                table: "input_audit");

            migrationBuilder.DropColumn(
                name: "pending_session_incarnation_id",
                table: "input_audit");
        }
    }
}
