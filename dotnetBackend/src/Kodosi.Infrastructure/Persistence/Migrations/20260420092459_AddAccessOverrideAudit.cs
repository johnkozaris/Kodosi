using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddAccessOverrideAudit : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "access_override_audit",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    session_id = table.Column<Guid>(type: "uuid", nullable: false),
                    actor_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    grantee_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    action = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    reason = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    occurred_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    client_ip = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: true),
                    user_agent = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_access_override_audit", x => x.id);
                });

            migrationBuilder.CreateIndex(
                name: "IX_access_override_audit_actor_user_id",
                table: "access_override_audit",
                column: "actor_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_access_override_audit_grantee_user_id",
                table: "access_override_audit",
                column: "grantee_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_access_override_audit_occurred_at",
                table: "access_override_audit",
                column: "occurred_at");

            migrationBuilder.CreateIndex(
                name: "IX_access_override_audit_session_id",
                table: "access_override_audit",
                column: "session_id");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "access_override_audit");
        }
    }
}
