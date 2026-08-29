using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class DropUserDeviceDeletedAt : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_user_devices_user_id_revoked_at_deleted_at",
                table: "user_devices");

            migrationBuilder.DropColumn(
                name: "deleted_at",
                table: "user_devices");

            migrationBuilder.CreateIndex(
                name: "IX_user_devices_user_id_revoked_at",
                table: "user_devices",
                columns: new[] { "user_id", "revoked_at" });
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_user_devices_user_id_revoked_at",
                table: "user_devices");

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "deleted_at",
                table: "user_devices",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.CreateIndex(
                name: "IX_user_devices_user_id_revoked_at_deleted_at",
                table: "user_devices",
                columns: new[] { "user_id", "revoked_at", "deleted_at" });
        }
    }
}
