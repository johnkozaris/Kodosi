using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddIdentityResetSessionTargets : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<string>(
                name: "ended_session_targets",
                table: "identity_reset_audit",
                type: "jsonb",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "session_targets",
                table: "identity_reset_audit",
                type: "jsonb",
                nullable: true);




            migrationBuilder.Sql(
                """
                UPDATE identity_reset_audit
                SET realtime_enforced_at = NOW()
                WHERE realtime_enforced_at IS NULL;
                """);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM identity_reset_audit
                        WHERE session_targets IS NOT NULL
                           OR ended_session_targets IS NOT NULL
                    ) THEN
                        RAISE EXCEPTION 'Cannot remove identity reset session targets while exact target audit evidence exists';
                    END IF;
                END $$;
                """);

            migrationBuilder.DropColumn(
                name: "ended_session_targets",
                table: "identity_reset_audit");

            migrationBuilder.DropColumn(
                name: "session_targets",
                table: "identity_reset_audit");
        }
    }
}
