using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class EnforceUserDeviceListProofPayload : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM user_device_lists
                        WHERE octet_length(body) = 0
                           OR octet_length(signature) = 0
                        LIMIT 1
                    ) THEN
                        RAISE EXCEPTION
                            'EnforceUserDeviceListProofPayload found a device list without a signed proof payload'
                            USING HINT = 'Re-enroll affected device lists from their original signed preimages before retrying.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.Sql(
                """
                ALTER TABLE user_device_lists
                    ALTER COLUMN body DROP DEFAULT;
                """);

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_device_lists_body_nonempty",
                table: "user_device_lists",
                sql: "octet_length(\"body\") > 0");

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_device_lists_signature_nonempty",
                table: "user_device_lists",
                sql: "octet_length(\"signature\") > 0");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_user_device_lists_body_nonempty",
                table: "user_device_lists");

            migrationBuilder.DropCheckConstraint(
                name: "CK_user_device_lists_signature_nonempty",
                table: "user_device_lists");
        }
    }
}
