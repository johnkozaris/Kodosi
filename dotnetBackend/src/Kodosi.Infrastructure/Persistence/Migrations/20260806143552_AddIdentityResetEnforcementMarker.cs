using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddIdentityResetEnforcementMarker : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<Guid[]>(
                name: "ended_session_ids",
                table: "identity_reset_audit",
                type: "uuid[]",
                nullable: false,
                defaultValue: new Guid[0]);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "realtime_enforced_at",
                table: "identity_reset_audit",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<string[]>(
                name: "removed_device_ids",
                table: "identity_reset_audit",
                type: "text[]",
                nullable: false,
                defaultValue: new string[0]);

            migrationBuilder.AddColumn<Guid[]>(
                name: "sessions_with_revoked_keys",
                table: "identity_reset_audit",
                type: "uuid[]",
                nullable: false,
                defaultValue: new Guid[0]);

            migrationBuilder.Sql(
                """
                UPDATE identity_reset_audit
                SET realtime_enforced_at = reset_at
                WHERE realtime_enforced_at IS NULL;
                """);

            migrationBuilder.CreateIndex(
                name: "IX_identity_reset_audit_realtime_enforced_at",
                table: "identity_reset_audit",
                column: "realtime_enforced_at");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_identity_reset_audit_realtime_enforced_at",
                table: "identity_reset_audit");

            migrationBuilder.DropColumn(
                name: "ended_session_ids",
                table: "identity_reset_audit");

            migrationBuilder.DropColumn(
                name: "realtime_enforced_at",
                table: "identity_reset_audit");

            migrationBuilder.DropColumn(
                name: "removed_device_ids",
                table: "identity_reset_audit");

            migrationBuilder.DropColumn(
                name: "sessions_with_revoked_keys",
                table: "identity_reset_audit");
        }
    }
}
