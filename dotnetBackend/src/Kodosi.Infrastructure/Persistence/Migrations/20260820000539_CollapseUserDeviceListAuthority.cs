using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class CollapseUserDeviceListAuthority : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_user_device_lists_device_ids_bounded",
                table: "user_device_lists");

            migrationBuilder.DropCheckConstraint(
                name: "CK_user_device_lists_issued_at_positive",
                table: "user_device_lists");

            migrationBuilder.DropColumn(
                name: "device_ids",
                table: "user_device_lists");

            migrationBuilder.DropColumn(
                name: "expires_at_ms",
                table: "user_device_lists");

            migrationBuilder.DropColumn(
                name: "issued_at_ms",
                table: "user_device_lists");

            migrationBuilder.DropColumn(
                name: "signer_device_id",
                table: "user_device_lists");

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_device_lists_body_bounded",
                table: "user_device_lists",
                sql: "octet_length(\"body\") <= 33687588");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (SELECT 1 FROM user_device_lists LIMIT 1) THEN
                        RAISE EXCEPTION
                            'CollapseUserDeviceListAuthority cannot reconstruct removed unsigned projections'
                            USING HINT = 'Restore from a backup taken before this migration instead of fabricating trust metadata.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.DropCheckConstraint(
                name: "CK_user_device_lists_body_bounded",
                table: "user_device_lists");

            migrationBuilder.AddColumn<string>(
                name: "device_ids",
                table: "user_device_lists",
                type: "jsonb",
                nullable: false,
                defaultValue: "");

            migrationBuilder.AddColumn<long>(
                name: "expires_at_ms",
                table: "user_device_lists",
                type: "bigint",
                nullable: true);

            migrationBuilder.AddColumn<long>(
                name: "issued_at_ms",
                table: "user_device_lists",
                type: "bigint",
                nullable: false,
                defaultValue: 0L);

            migrationBuilder.AddColumn<string>(
                name: "signer_device_id",
                table: "user_device_lists",
                type: "character varying(256)",
                maxLength: 256,
                nullable: false,
                defaultValue: "");

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_device_lists_device_ids_bounded",
                table: "user_device_lists",
                sql: "length(\"device_ids\"::text) < 65536");

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_device_lists_issued_at_positive",
                table: "user_device_lists",
                sql: "\"issued_at_ms\" > 0");
        }
    }
}
