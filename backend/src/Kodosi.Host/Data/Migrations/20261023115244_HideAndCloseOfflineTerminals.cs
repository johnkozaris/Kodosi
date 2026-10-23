using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class HideAndCloseOfflineTerminals : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<string>(
                name: "CloseDeviceId",
                table: "sessions",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "CloseSignature",
                table: "sessions",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<Guid>(
                name: "CloseUserId",
                table: "sessions",
                type: "uuid",
                nullable: true);

            migrationBuilder.CreateTable(
                name: "hidden_sessions",
                columns: table => new
                {
                    UserId = table.Column<Guid>(type: "uuid", nullable: false),
                    SessionId = table.Column<Guid>(type: "uuid", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_hidden_sessions", x => new { x.UserId, x.SessionId });
                    table.ForeignKey(
                        name: "FK_hidden_sessions_sessions_SessionId",
                        column: x => x.SessionId,
                        principalTable: "sessions",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                    table.ForeignKey(
                        name: "FK_hidden_sessions_users_UserId",
                        column: x => x.UserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateIndex(
                name: "IX_hidden_sessions_SessionId",
                table: "hidden_sessions",
                column: "SessionId");
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "hidden_sessions");

            migrationBuilder.DropColumn(
                name: "CloseDeviceId",
                table: "sessions");

            migrationBuilder.DropColumn(
                name: "CloseSignature",
                table: "sessions");

            migrationBuilder.DropColumn(
                name: "CloseUserId",
                table: "sessions");
        }
    }
}
