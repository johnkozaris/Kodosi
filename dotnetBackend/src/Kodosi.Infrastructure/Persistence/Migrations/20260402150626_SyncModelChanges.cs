using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class SyncModelChanges : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "session_key_blobs",
                columns: table => new
                {
                    coding_session_id = table.Column<Guid>(type: "uuid", nullable: false),
                    recipient_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    encrypted_session_key = table.Column<byte[]>(type: "bytea", nullable: false),
                    sender_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    key_generation = table.Column<int>(type: "integer", nullable: false),
                    signature = table.Column<byte[]>(type: "bytea", nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_session_key_blobs", x => new { x.coding_session_id, x.recipient_device_id });
                });

            migrationBuilder.CreateTable(
                name: "user_devices",
                columns: table => new
                {
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    kem_public_key = table.Column<byte[]>(type: "bytea", maxLength: 1184, nullable: false),
                    signing_public_key = table.Column<byte[]>(type: "bytea", maxLength: 1952, nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_user_devices", x => new { x.user_id, x.device_id });
                });

            migrationBuilder.CreateIndex(
                name: "IX_session_key_blobs_coding_session_id",
                table: "session_key_blobs",
                column: "coding_session_id");

            migrationBuilder.CreateIndex(
                name: "IX_user_devices_device_id",
                table: "user_devices",
                column: "device_id",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_user_devices_user_id",
                table: "user_devices",
                column: "user_id");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "session_key_blobs");

            migrationBuilder.DropTable(
                name: "user_devices");
        }
    }
}
