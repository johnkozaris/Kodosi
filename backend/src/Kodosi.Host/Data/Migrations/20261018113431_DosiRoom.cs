using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class DosiRoom : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<bool>(
                name: "Dosi",
                table: "missions",
                type: "boolean",
                nullable: false,
                defaultValue: false);

            migrationBuilder.CreateIndex(
                name: "IX_missions_Dosi_OwnerUserId",
                table: "missions",
                columns: new[] { "Dosi", "OwnerUserId" },
                unique: true,
                filter: "\"Dosi\"");
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_missions_Dosi_OwnerUserId",
                table: "missions");

            migrationBuilder.DropColumn(
                name: "Dosi",
                table: "missions");
        }
    }
}
