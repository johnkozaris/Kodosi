using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class WidenDeviceRevocationGeneration : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AlterColumn<long>(
                name: "new_generation",
                table: "device_revocation_audit",
                type: "bigint",
                nullable: false,
                oldClrType: typeof(int),
                oldType: "integer");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM device_revocation_audit
                        WHERE new_generation > 2147483647
                        LIMIT 1
                    ) THEN
                        RAISE EXCEPTION
                            'WidenDeviceRevocationGeneration cannot narrow deployed generation evidence'
                            USING HINT = 'Retain the bigint column or remove no-longer-required audit rows under an explicit retention policy.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.AlterColumn<int>(
                name: "new_generation",
                table: "device_revocation_audit",
                type: "integer",
                nullable: false,
                oldClrType: typeof(long),
                oldType: "bigint");
        }
    }
}
