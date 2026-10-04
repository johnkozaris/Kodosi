using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class RelayTerminalChannels : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "session_keys");

            migrationBuilder.DropColumn(
                name: "KeyGeneration",
                table: "sessions");

            migrationBuilder.DropColumn(
                name: "Ready",
                table: "sessions");
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<int>(
                name: "KeyGeneration",
                table: "sessions",
                type: "integer",
                nullable: false,
                defaultValue: 0);

            migrationBuilder.AddColumn<bool>(
                name: "Ready",
                table: "sessions",
                type: "boolean",
                nullable: false,
                defaultValue: false);

            migrationBuilder.CreateTable(
                name: "session_keys",
                columns: table => new
                {
                    SessionId = table.Column<Guid>(type: "uuid", nullable: false),
                    RecipientDeviceId = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    EncryptedKey = table.Column<byte[]>(type: "bytea", nullable: false),
                    IssuedAtMs = table.Column<long>(type: "bigint", nullable: false),
                    KeyGeneration = table.Column<int>(type: "integer", nullable: false),
                    RecipientUserId = table.Column<Guid>(type: "uuid", nullable: false),
                    SenderDeviceId = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    Signature = table.Column<byte[]>(type: "bytea", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_session_keys", x => new { x.SessionId, x.RecipientDeviceId });
                    table.ForeignKey(
                        name: "FK_session_keys_devices_RecipientDeviceId",
                        column: x => x.RecipientDeviceId,
                        principalTable: "devices",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "FK_session_keys_sessions_SessionId",
                        column: x => x.SessionId,
                        principalTable: "sessions",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateIndex(
                name: "IX_session_keys_RecipientDeviceId",
                table: "session_keys",
                column: "RecipientDeviceId");
        }
    }
}
