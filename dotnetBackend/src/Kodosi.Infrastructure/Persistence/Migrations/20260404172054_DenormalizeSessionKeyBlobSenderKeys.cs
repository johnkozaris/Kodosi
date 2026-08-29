using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class DenormalizeSessionKeyBlobSenderKeys : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<byte[]>(
                name: "sender_kem_public_key",
                table: "session_key_blobs",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);

            migrationBuilder.AddColumn<byte[]>(
                name: "sender_signing_public_key",
                table: "session_key_blobs",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropColumn(
                name: "sender_kem_public_key",
                table: "session_key_blobs");

            migrationBuilder.DropColumn(
                name: "sender_signing_public_key",
                table: "session_key_blobs");
        }
    }
}
