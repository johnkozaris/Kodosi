using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddPermissionRequestGenerationFence : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "ck_input_audit_permission_pending_tuple",
                table: "input_audit");

            migrationBuilder.AddColumn<long>(
                name: "pending_request_generation",
                table: "input_audit",
                type: "bigint",
                nullable: true);

            migrationBuilder.Sql("""
                UPDATE input_audit
                SET status = CASE WHEN status = 'Pending' THEN 'Failed' ELSE status END,
                    completed_at = CASE
                        WHEN status = 'Pending' THEN CURRENT_TIMESTAMP
                        ELSE completed_at
                    END,
                    pending_session_incarnation_id = NULL,
                    pending_session_incarnation_generation = NULL,
                    pending_request_id = NULL,
                    pending_request_generation = NULL,
                    pending_requester_device_id = NULL
                WHERE kind = 'PermissionDecision'
                  AND pending_request_id IS NOT NULL;
                """);

            migrationBuilder.AddCheckConstraint(
                name: "ck_input_audit_permission_pending_tuple",
                table: "input_audit",
                sql: "(pending_session_incarnation_id IS NULL AND pending_session_incarnation_generation IS NULL AND pending_request_id IS NULL AND pending_request_generation IS NULL AND pending_requester_device_id IS NULL) OR (pending_session_incarnation_id IS NOT NULL AND pending_session_incarnation_generation IS NOT NULL AND pending_session_incarnation_generation > 0 AND pending_request_id IS NOT NULL AND pending_request_generation IS NOT NULL AND pending_request_generation > 0 AND pending_requester_device_id IS NOT NULL)");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "ck_input_audit_permission_pending_tuple",
                table: "input_audit");

            migrationBuilder.DropColumn(
                name: "pending_request_generation",
                table: "input_audit");

            migrationBuilder.AddCheckConstraint(
                name: "ck_input_audit_permission_pending_tuple",
                table: "input_audit",
                sql: "(pending_session_incarnation_id IS NULL AND pending_session_incarnation_generation IS NULL AND pending_request_id IS NULL AND pending_requester_device_id IS NULL) OR (pending_session_incarnation_id IS NOT NULL AND pending_session_incarnation_generation IS NOT NULL AND pending_session_incarnation_generation > 0 AND pending_request_id IS NOT NULL AND pending_requester_device_id IS NOT NULL)");
        }
    }
}
