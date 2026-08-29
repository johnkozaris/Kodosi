using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class RemovePublicSessionScope : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {


            migrationBuilder.Sql(
                """
                DELETE FROM session_key_blobs
                WHERE session_id IN (SELECT id FROM sessions WHERE scope = 'Public');
                """);

            migrationBuilder.Sql(
                """
                DELETE FROM session_access_overrides
                WHERE session_id IN (SELECT id FROM sessions WHERE scope = 'Public');
                """);

            migrationBuilder.Sql(
                """
                DELETE FROM input_audit
                WHERE session_id IN (SELECT id FROM sessions WHERE scope = 'Public');
                """);

            migrationBuilder.Sql(
                """
                DELETE FROM sessions WHERE scope = 'Public';
                """);

            migrationBuilder.DropCheckConstraint(
                name: "CK_sessions_public_scope_is_view_only",
                table: "sessions");

            migrationBuilder.AddCheckConstraint(
                name: "CK_sessions_scope_allowed_values",
                table: "sessions",
                sql: "\"scope\" IN ('Private', 'Friends', 'Workspace')");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_sessions_scope_allowed_values",
                table: "sessions");

            migrationBuilder.AddCheckConstraint(
                name: "CK_sessions_public_scope_is_view_only",
                table: "sessions",
                sql: "\"scope\" <> 'Public' OR \"default_access\" = 'View'");
        }
    }
}
