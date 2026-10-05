using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class RemoveDeviceKemKeys : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropColumn(
                name: "KemPublicKey",
                table: "devices");

            migrationBuilder.DropColumn(
                name: "KemPublicKey",
                table: "device_links");
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<byte[]>(
                name: "KemPublicKey",
                table: "devices",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);

            migrationBuilder.AddColumn<byte[]>(
                name: "KemPublicKey",
                table: "device_links",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);
        }
    }
}
