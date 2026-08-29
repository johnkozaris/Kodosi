using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddAccessOverrideExpiryEnforcement : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "expected_expires_at",
                table: "access_override_audit",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "expected_revoked_at",
                table: "access_override_audit",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "realtime_enforced_at",
                table: "access_override_audit",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<Guid>(
                name: "session_incarnation_id",
                table: "access_override_audit",
                type: "uuid",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "session_started_at",
                table: "access_override_audit",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.Sql(
                """
                UPDATE access_override_audit
                SET realtime_enforced_at = occurred_at
                WHERE realtime_enforced_at IS NULL;
                """);

            migrationBuilder.CreateIndex(
                name: "IX_access_override_audit_occurred_at_id",
                table: "access_override_audit",
                columns: new[] { "occurred_at", "id" },
                filter: "realtime_enforced_at IS NULL");

            migrationBuilder.AddCheckConstraint(
                name: "CK_access_override_audit_pending_expiry_evidence",
                table: "access_override_audit",
                sql: "realtime_enforced_at IS NOT NULL OR (action = 'Revoked' AND reason = 'Expired' AND session_incarnation_id IS NOT NULL AND session_started_at IS NOT NULL AND expected_expires_at IS NOT NULL AND expected_revoked_at IS NOT NULL)");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM access_override_audit
                        WHERE session_incarnation_id IS NOT NULL
                           OR session_started_at IS NOT NULL
                           OR expected_expires_at IS NOT NULL
                           OR expected_revoked_at IS NOT NULL
                           OR realtime_enforced_at IS DISTINCT FROM occurred_at
                    ) THEN
                        RAISE EXCEPTION 'Cannot remove access override expiry enforcement after exact enforcement evidence exists';
                    END IF;
                END $$;
                """);

            migrationBuilder.DropIndex(
                name: "IX_access_override_audit_occurred_at_id",
                table: "access_override_audit");

            migrationBuilder.DropCheckConstraint(
                name: "CK_access_override_audit_pending_expiry_evidence",
                table: "access_override_audit");

            migrationBuilder.DropColumn(
                name: "expected_expires_at",
                table: "access_override_audit");

            migrationBuilder.DropColumn(
                name: "expected_revoked_at",
                table: "access_override_audit");

            migrationBuilder.DropColumn(
                name: "realtime_enforced_at",
                table: "access_override_audit");

            migrationBuilder.DropColumn(
                name: "session_incarnation_id",
                table: "access_override_audit");

            migrationBuilder.DropColumn(
                name: "session_started_at",
                table: "access_override_audit");
        }
    }
}
