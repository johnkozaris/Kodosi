using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddFriendshipWorkspaceDeviceRevocationAudit : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "device_revocation_audit",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    actor_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    revoked_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    signer_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    new_generation = table.Column<int>(type: "integer", nullable: false),
                    blobs_cascaded = table.Column<int>(type: "integer", nullable: false),
                    occurred_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    client_ip = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: true),
                    user_agent = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_device_revocation_audit", x => x.id);
                });

            migrationBuilder.CreateTable(
                name: "friendship_audit",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    actor_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    other_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    action = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    occurred_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    client_ip = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: true),
                    user_agent = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_friendship_audit", x => x.id);
                });

            migrationBuilder.CreateTable(
                name: "workspace_member_audit",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    workspace_id = table.Column<Guid>(type: "uuid", nullable: false),
                    actor_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    target_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    action = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    occurred_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    client_ip = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: true),
                    user_agent = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_workspace_member_audit", x => x.id);
                });

            migrationBuilder.CreateIndex(
                name: "IX_device_revocation_audit_actor_user_id",
                table: "device_revocation_audit",
                column: "actor_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_device_revocation_audit_occurred_at",
                table: "device_revocation_audit",
                column: "occurred_at");

            migrationBuilder.CreateIndex(
                name: "IX_device_revocation_audit_revoked_device_id",
                table: "device_revocation_audit",
                column: "revoked_device_id");

            migrationBuilder.CreateIndex(
                name: "IX_friendship_audit_actor_user_id",
                table: "friendship_audit",
                column: "actor_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_friendship_audit_occurred_at",
                table: "friendship_audit",
                column: "occurred_at");

            migrationBuilder.CreateIndex(
                name: "IX_friendship_audit_other_user_id",
                table: "friendship_audit",
                column: "other_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_workspace_member_audit_actor_user_id",
                table: "workspace_member_audit",
                column: "actor_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_workspace_member_audit_occurred_at",
                table: "workspace_member_audit",
                column: "occurred_at");

            migrationBuilder.CreateIndex(
                name: "IX_workspace_member_audit_target_user_id",
                table: "workspace_member_audit",
                column: "target_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_workspace_member_audit_workspace_id",
                table: "workspace_member_audit",
                column: "workspace_id");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "device_revocation_audit");

            migrationBuilder.DropTable(
                name: "friendship_audit");

            migrationBuilder.DropTable(
                name: "workspace_member_audit");
        }
    }
}
