using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AlignRoomSlugMaxLength : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM rooms
                        WHERE char_length(slug) > 64
                        LIMIT 1
                    ) THEN
                        RAISE EXCEPTION
                            'AlignRoomSlugMaxLength cannot narrow rooms.slug while values longer than 64 characters exist'
                            USING HINT = 'Rename every room whose slug is 65-128 characters, then retry the migration.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.AlterColumn<string>(
                name: "slug",
                table: "rooms",
                type: "character varying(64)",
                maxLength: 64,
                nullable: false,
                oldClrType: typeof(string),
                oldType: "character varying(128)",
                oldMaxLength: 128);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AlterColumn<string>(
                name: "slug",
                table: "rooms",
                type: "character varying(128)",
                maxLength: 128,
                nullable: false,
                oldClrType: typeof(string),
                oldType: "character varying(64)",
                oldMaxLength: 64);
        }
    }
}
