using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class EnforceSessionScopeInvariants : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                UPDATE coding_sessions
                SET workspace_id = NULL
                WHERE scope <> 'Workspace' AND workspace_id IS NOT NULL;
                """);

            migrationBuilder.Sql(
                """
                UPDATE coding_sessions
                SET default_access = 'View'
                WHERE scope = 'Public' AND default_access <> 'View';
                """);

            migrationBuilder.Sql(
                """
                DO $$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM coding_sessions
                        WHERE scope = 'Workspace' AND workspace_id IS NULL
                    ) THEN
                        RAISE EXCEPTION
                            'Cannot enforce coding session scope invariants while workspace-scoped sessions are missing workspace_id.';
                    END IF;
                END
                $$;
                """);

            migrationBuilder.AddCheckConstraint(
                name: "CK_coding_sessions_public_scope_is_view_only",
                table: "coding_sessions",
                sql: "\"scope\" <> 'Public' OR \"default_access\" = 'View'");

            migrationBuilder.AddCheckConstraint(
                name: "CK_coding_sessions_workspace_scope_matches_workspace_id",
                table: "coding_sessions",
                sql: "(\"scope\" = 'Workspace' AND \"workspace_id\" IS NOT NULL)\nOR (\"scope\" <> 'Workspace' AND \"workspace_id\" IS NULL)");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_coding_sessions_public_scope_is_view_only",
                table: "coding_sessions");

            migrationBuilder.DropCheckConstraint(
                name: "CK_coding_sessions_workspace_scope_matches_workspace_id",
                table: "coding_sessions");
        }
    }
}
