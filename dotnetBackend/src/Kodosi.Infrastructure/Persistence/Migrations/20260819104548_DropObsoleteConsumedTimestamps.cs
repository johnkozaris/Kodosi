using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class DropObsoleteConsumedTimestamps : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql("""
                UPDATE device_link_requests
                SET acknowledged_at = consumed_at
                WHERE acknowledged_at IS NULL
                  AND consumed_at IS NOT NULL;
                """);

            migrationBuilder.DropColumn(
                name: "consumed_at",
                table: "device_registration_challenges");

            migrationBuilder.DropColumn(
                name: "consumed_at",
                table: "device_link_requests");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "consumed_at",
                table: "device_registration_challenges",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "consumed_at",
                table: "device_link_requests",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.Sql("""
                UPDATE device_link_requests
                SET consumed_at = acknowledged_at
                WHERE acknowledged_at IS NOT NULL;
                """);
        }
    }
}
