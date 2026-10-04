using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class TypedDeviceLinkCode : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql("DELETE FROM device_links;");

            migrationBuilder.DropIndex(
                name: "IX_device_links_UserCode",
                table: "device_links");

            migrationBuilder.DropColumn(
                name: "UserCode",
                table: "device_links");

            migrationBuilder.AddColumn<byte[]>(
                name: "ApprovalProof",
                table: "device_links",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "Nonce",
                table: "device_links",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);

            migrationBuilder.AddColumn<byte[]>(
                name: "Proof",
                table: "device_links",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql("DELETE FROM device_links;");

            migrationBuilder.DropColumn(
                name: "ApprovalProof",
                table: "device_links");

            migrationBuilder.DropColumn(
                name: "Nonce",
                table: "device_links");

            migrationBuilder.DropColumn(
                name: "Proof",
                table: "device_links");

            migrationBuilder.AddColumn<string>(
                name: "UserCode",
                table: "device_links",
                type: "character varying(9)",
                maxLength: 9,
                nullable: false,
                defaultValue: "");

            migrationBuilder.CreateIndex(
                name: "IX_device_links_UserCode",
                table: "device_links",
                column: "UserCode",
                unique: true);
        }
    }
}
