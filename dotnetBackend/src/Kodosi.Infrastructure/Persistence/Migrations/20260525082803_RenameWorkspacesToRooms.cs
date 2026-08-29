using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class RenameWorkspacesToRooms : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropForeignKey(
                name: "FK_sessions_workspaces_workspace_id",
                table: "sessions");

            migrationBuilder.DropForeignKey(
                name: "FK_workspace_members_workspaces_workspace_id",
                table: "workspace_members");

            migrationBuilder.DropForeignKey(
                name: "FK_workspace_members_users_user_id",
                table: "workspace_members");

            migrationBuilder.DropForeignKey(
                name: "FK_workspaces_users_owner_user_id",
                table: "workspaces");

            migrationBuilder.DropCheckConstraint(
                name: "CK_sessions_workspace_scope_matches_workspace_id",
                table: "sessions");

            migrationBuilder.DropPrimaryKey(
                name: "PK_workspaces",
                table: "workspaces");

            migrationBuilder.DropPrimaryKey(
                name: "PK_workspace_members",
                table: "workspace_members");

            migrationBuilder.RenameTable(
                name: "workspaces",
                newName: "rooms");

            migrationBuilder.RenameTable(
                name: "workspace_members",
                newName: "room_members");

            migrationBuilder.RenameTable(
                name: "workspace_member_audit",
                newName: "room_member_audit");

            migrationBuilder.RenameColumn(
                name: "workspace_id",
                table: "sessions",
                newName: "room_id");

            migrationBuilder.RenameColumn(
                name: "workspace_id",
                table: "room_members",
                newName: "room_id");

            migrationBuilder.RenameColumn(
                name: "workspace_id",
                table: "room_member_audit",
                newName: "room_id");

            migrationBuilder.RenameIndex(
                name: "IX_sessions_workspace_id",
                table: "sessions",
                newName: "IX_sessions_room_id");

            migrationBuilder.RenameIndex(
                name: "IX_workspaces_slug",
                table: "rooms",
                newName: "IX_rooms_slug");

            migrationBuilder.RenameIndex(
                name: "IX_workspaces_owner_user_id",
                table: "rooms",
                newName: "IX_rooms_owner_user_id");

            migrationBuilder.RenameIndex(
                name: "IX_workspace_members_user_id_revoked_at",
                table: "room_members",
                newName: "IX_room_members_user_id_revoked_at");

            migrationBuilder.RenameIndex(
                name: "IX_workspace_member_audit_actor_user_id",
                table: "room_member_audit",
                newName: "IX_room_member_audit_actor_user_id");

            migrationBuilder.RenameIndex(
                name: "IX_workspace_member_audit_occurred_at",
                table: "room_member_audit",
                newName: "IX_room_member_audit_occurred_at");

            migrationBuilder.RenameIndex(
                name: "IX_workspace_member_audit_target_user_id",
                table: "room_member_audit",
                newName: "IX_room_member_audit_target_user_id");

            migrationBuilder.RenameIndex(
                name: "IX_workspace_member_audit_workspace_id",
                table: "room_member_audit",
                newName: "IX_room_member_audit_room_id");




            migrationBuilder.DropCheckConstraint(
                name: "CK_sessions_scope_allowed_values",
                table: "sessions");

            migrationBuilder.Sql(
                """
                UPDATE sessions SET scope = 'Room' WHERE scope = 'Workspace';
                """);

            migrationBuilder.AddCheckConstraint(
                name: "CK_sessions_scope_allowed_values",
                table: "sessions",
                sql: "\"scope\" IN ('JustMe', 'MyDevices', 'Friends', 'Room')");

            migrationBuilder.AddPrimaryKey(
                name: "PK_rooms",
                table: "rooms",
                column: "id");

            migrationBuilder.AddPrimaryKey(
                name: "PK_room_members",
                table: "room_members",
                columns: new[] { "room_id", "user_id" });

            migrationBuilder.AddCheckConstraint(
                name: "CK_sessions_room_scope_matches_room_id",
                table: "sessions",
                sql: "(\"scope\" = 'Room' AND \"room_id\" IS NOT NULL)\nOR (\"scope\" <> 'Room' AND \"room_id\" IS NULL)");

            migrationBuilder.AddForeignKey(
                name: "FK_rooms_users_owner_user_id",
                table: "rooms",
                column: "owner_user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_sessions_rooms_room_id",
                table: "sessions",
                column: "room_id",
                principalTable: "rooms",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_room_members_rooms_room_id",
                table: "room_members",
                column: "room_id",
                principalTable: "rooms",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_room_members_users_user_id",
                table: "room_members",
                column: "user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {


            migrationBuilder.DropForeignKey(
                name: "FK_rooms_users_owner_user_id",
                table: "rooms");

            migrationBuilder.DropForeignKey(
                name: "FK_sessions_rooms_room_id",
                table: "sessions");

            migrationBuilder.DropForeignKey(
                name: "FK_room_members_rooms_room_id",
                table: "room_members");

            migrationBuilder.DropForeignKey(
                name: "FK_room_members_users_user_id",
                table: "room_members");

            migrationBuilder.DropCheckConstraint(
                name: "CK_sessions_room_scope_matches_room_id",
                table: "sessions");

            migrationBuilder.DropPrimaryKey(
                name: "PK_rooms",
                table: "rooms");

            migrationBuilder.DropPrimaryKey(
                name: "PK_room_members",
                table: "room_members");

            migrationBuilder.RenameTable(name: "rooms", newName: "workspaces");
            migrationBuilder.RenameTable(name: "room_members", newName: "workspace_members");
            migrationBuilder.RenameTable(name: "room_member_audit", newName: "workspace_member_audit");

            migrationBuilder.RenameColumn(name: "room_id", table: "sessions", newName: "workspace_id");
            migrationBuilder.RenameColumn(name: "room_id", table: "workspace_members", newName: "workspace_id");
            migrationBuilder.RenameColumn(name: "room_id", table: "workspace_member_audit", newName: "workspace_id");

            migrationBuilder.RenameIndex(name: "IX_sessions_room_id", table: "sessions", newName: "IX_sessions_workspace_id");
            migrationBuilder.RenameIndex(name: "IX_rooms_slug", table: "workspaces", newName: "IX_workspaces_slug");
            migrationBuilder.RenameIndex(name: "IX_rooms_owner_user_id", table: "workspaces", newName: "IX_workspaces_owner_user_id");
            migrationBuilder.RenameIndex(name: "IX_room_members_user_id_revoked_at", table: "workspace_members", newName: "IX_workspace_members_user_id_revoked_at");
            migrationBuilder.RenameIndex(name: "IX_room_member_audit_actor_user_id", table: "workspace_member_audit", newName: "IX_workspace_member_audit_actor_user_id");
            migrationBuilder.RenameIndex(name: "IX_room_member_audit_occurred_at", table: "workspace_member_audit", newName: "IX_workspace_member_audit_occurred_at");
            migrationBuilder.RenameIndex(name: "IX_room_member_audit_target_user_id", table: "workspace_member_audit", newName: "IX_workspace_member_audit_target_user_id");
            migrationBuilder.RenameIndex(name: "IX_room_member_audit_room_id", table: "workspace_member_audit", newName: "IX_workspace_member_audit_workspace_id");

            migrationBuilder.DropCheckConstraint(
                name: "CK_sessions_scope_allowed_values",
                table: "sessions");

            migrationBuilder.Sql(
                """
                UPDATE sessions SET scope = 'Workspace' WHERE scope = 'Room';
                """);

            migrationBuilder.AddCheckConstraint(
                name: "CK_sessions_scope_allowed_values",
                table: "sessions",
                sql: "\"scope\" IN ('JustMe', 'MyDevices', 'Friends', 'Workspace')");

            migrationBuilder.AddPrimaryKey(name: "PK_workspaces", table: "workspaces", column: "id");
            migrationBuilder.AddPrimaryKey(name: "PK_workspace_members", table: "workspace_members", columns: new[] { "workspace_id", "user_id" });

            migrationBuilder.AddCheckConstraint(
                name: "CK_sessions_workspace_scope_matches_workspace_id",
                table: "sessions",
                sql: "(\"scope\" = 'Workspace' AND \"workspace_id\" IS NOT NULL)\nOR (\"scope\" <> 'Workspace' AND \"workspace_id\" IS NULL)");

            migrationBuilder.AddForeignKey(name: "FK_workspaces_users_owner_user_id", table: "workspaces", column: "owner_user_id", principalTable: "users", principalColumn: "id", onDelete: ReferentialAction.Restrict);
            migrationBuilder.AddForeignKey(name: "FK_sessions_workspaces_workspace_id", table: "sessions", column: "workspace_id", principalTable: "workspaces", principalColumn: "id", onDelete: ReferentialAction.Restrict);
            migrationBuilder.AddForeignKey(name: "FK_workspace_members_workspaces_workspace_id", table: "workspace_members", column: "workspace_id", principalTable: "workspaces", principalColumn: "id", onDelete: ReferentialAction.Restrict);
            migrationBuilder.AddForeignKey(name: "FK_workspace_members_users_user_id", table: "workspace_members", column: "user_id", principalTable: "users", principalColumn: "id", onDelete: ReferentialAction.Restrict);
        }
    }
}
