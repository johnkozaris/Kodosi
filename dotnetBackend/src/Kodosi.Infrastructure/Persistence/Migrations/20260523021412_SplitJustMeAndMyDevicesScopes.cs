using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class SplitJustMeAndMyDevicesScopes : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_sessions_scope_allowed_values",
                table: "sessions");

            migrationBuilder.Sql(
                """
                UPDATE sessions SET scope = 'JustMe' WHERE scope = 'Private';
                """);

            migrationBuilder.AddCheckConstraint(
                name: "CK_sessions_scope_allowed_values",
                table: "sessions",
                sql: "\"scope\" IN ('JustMe', 'MyDevices', 'Friends', 'Workspace')");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_sessions_scope_allowed_values",
                table: "sessions");


            migrationBuilder.Sql(
                """
                UPDATE sessions SET scope = 'JustMe' WHERE scope = 'MyDevices';
                """);

            migrationBuilder.Sql(
                """
                UPDATE sessions SET scope = 'Private' WHERE scope = 'JustMe';
                """);

            migrationBuilder.AddCheckConstraint(
                name: "CK_sessions_scope_allowed_values",
                table: "sessions",
                sql: "\"scope\" IN ('Private', 'Friends', 'Workspace')");
        }
    }
}
