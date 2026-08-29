using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddIdentityResetAudit : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "identity_reset_audit",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    reset_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    sessions_attempted = table.Column<int>(type: "integer", nullable: false),
                    sessions_ended = table.Column<int>(type: "integer", nullable: false),
                    devices_removed = table.Column<int>(type: "integer", nullable: false),
                    device_lists_removed = table.Column<int>(type: "integer", nullable: false),
                    client_ip = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: true),
                    user_agent = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_identity_reset_audit", x => x.id);
                });

            migrationBuilder.CreateIndex(
                name: "IX_identity_reset_audit_reset_at",
                table: "identity_reset_audit",
                column: "reset_at");

            migrationBuilder.CreateIndex(
                name: "IX_identity_reset_audit_user_id",
                table: "identity_reset_audit",
                column: "user_id");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "identity_reset_audit");
        }
    }
}
