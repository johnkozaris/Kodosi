using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class Plans : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<string>(
                name: "Plan",
                table: "users",
                type: "character varying(64)",
                maxLength: 64,
                nullable: true);

            migrationBuilder.AddColumn<long>(
                name: "ContentBytes",
                table: "missions",
                type: "bigint",
                nullable: false,
                defaultValue: 0L);

            migrationBuilder.AddColumn<bool>(
                name: "HistoryTrimmed",
                table: "missions",
                type: "boolean",
                nullable: false,
                defaultValue: false);

            migrationBuilder.Sql("""
                UPDATE missions AS m SET "ContentBytes" = COALESCE((SELECT SUM(octet_length(i."Body") + octet_length(i."Signature"))
                    FROM mission_items AS i WHERE i."MissionId" = m."Id"), 0);
                """);
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropColumn(
                name: "Plan",
                table: "users");

            migrationBuilder.DropColumn(
                name: "ContentBytes",
                table: "missions");

            migrationBuilder.DropColumn(
                name: "HistoryTrimmed",
                table: "missions");
        }
    }
}
