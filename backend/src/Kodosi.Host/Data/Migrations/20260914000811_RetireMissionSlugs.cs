using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    public partial class RetireMissionSlugs : Migration
    {
            protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_rooms_Slug",
                table: "rooms");

            migrationBuilder.AlterColumn<string>(
                name: "Slug",
                table: "rooms",
                type: "character varying(64)",
                maxLength: 64,
                nullable: true,
                oldClrType: typeof(string),
                oldType: "character varying(64)",
                oldMaxLength: 64);
        }

            protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql("""
                DO $$ BEGIN
                    IF EXISTS (SELECT 1 FROM rooms WHERE "Slug" IS NULL)
                       OR EXISTS (SELECT "Slug" FROM rooms GROUP BY "Slug" HAVING COUNT(*) > 1)
                    THEN RAISE EXCEPTION 'Cannot restore Mission slug constraints without an explicit data migration.';
                    END IF;
                END $$;
                """);
            migrationBuilder.AlterColumn<string>(
                name: "Slug",
                table: "rooms",
                type: "character varying(64)",
                maxLength: 64,
                nullable: false,
                oldClrType: typeof(string),
                oldType: "character varying(64)",
                oldMaxLength: 64,
                oldNullable: true);

            migrationBuilder.CreateIndex(
                name: "IX_rooms_Slug",
                table: "rooms",
                column: "Slug",
                unique: true);
        }
    }
}
