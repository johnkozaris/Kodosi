using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddDeviceRevocationSessionTargets : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<string>(
                name: "affected_session_targets",
                table: "device_revocation_audit",
                type: "jsonb",
                nullable: true);




            migrationBuilder.Sql(
                """
                UPDATE device_revocation_audit
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
                        FROM device_revocation_audit
                        WHERE affected_session_targets IS NOT NULL
                    ) THEN
                        RAISE EXCEPTION 'Cannot remove device revocation session targets while exact target audit evidence exists';
                    END IF;
                END $$;
                """);

            migrationBuilder.DropColumn(
                name: "affected_session_targets",
                table: "device_revocation_audit");
        }
    }
}
