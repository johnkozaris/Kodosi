using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class HardenAuditAndDeviceLinkConcurrency : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<int>(
                name: "duplicate_count",
                table: "input_audit",
                type: "integer",
                nullable: false,
                defaultValue: 0);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "last_duplicate_at",
                table: "input_audit",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<uint>(
                name: "xmin",
                table: "device_link_requests",
                type: "xid",
                rowVersion: true,
                nullable: false,
                defaultValue: 0u);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropColumn(
                name: "duplicate_count",
                table: "input_audit");

            migrationBuilder.DropColumn(
                name: "last_duplicate_at",
                table: "input_audit");

            migrationBuilder.DropColumn(
                name: "xmin",
                table: "device_link_requests");
        }
    }
}
