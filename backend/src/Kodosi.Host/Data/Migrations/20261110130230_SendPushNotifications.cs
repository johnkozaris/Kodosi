using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class SendPushNotifications : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "push_tokens",
                columns: table => new
                {
                    DeviceId = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    UserId = table.Column<Guid>(type: "uuid", nullable: false),
                    Platform = table.Column<string>(type: "character varying(8)", maxLength: 8, nullable: false),
                    Token = table.Column<string>(type: "character varying(4096)", maxLength: 4096, nullable: false),
                    Environment = table.Column<string>(type: "character varying(16)", maxLength: 16, nullable: true),
                    UpdatedAt = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_push_tokens", x => x.DeviceId);
                    table.ForeignKey(
                        name: "FK_push_tokens_devices_DeviceId",
                        column: x => x.DeviceId,
                        principalTable: "devices",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                    table.ForeignKey(
                        name: "FK_push_tokens_users_UserId",
                        column: x => x.UserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateIndex(
                name: "IX_push_tokens_Token",
                table: "push_tokens",
                column: "Token")
                .Annotation("Npgsql:IndexMethod", "hash");

            migrationBuilder.CreateIndex(
                name: "IX_push_tokens_UserId",
                table: "push_tokens",
                column: "UserId");
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "push_tokens");
        }
    }
}
