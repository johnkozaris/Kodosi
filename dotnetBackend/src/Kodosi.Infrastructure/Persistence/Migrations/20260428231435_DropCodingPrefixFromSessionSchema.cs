using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class DropCodingPrefixFromSessionSchema : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropForeignKey(
                name: "FK_coding_session_access_overrides_coding_sessions_coding_sess~",
                table: "coding_session_access_overrides");

            migrationBuilder.DropForeignKey(
                name: "FK_coding_session_access_overrides_users_actor_user_id",
                table: "coding_session_access_overrides");

            migrationBuilder.DropForeignKey(
                name: "FK_coding_session_access_overrides_users_granted_by_user_id",
                table: "coding_session_access_overrides");

            migrationBuilder.DropForeignKey(
                name: "FK_coding_sessions_users_owner_user_id",
                table: "coding_sessions");

            migrationBuilder.DropForeignKey(
                name: "FK_coding_sessions_workspaces_workspace_id",
                table: "coding_sessions");

            migrationBuilder.DropForeignKey(
                name: "FK_input_audit_coding_sessions_coding_session_id",
                table: "input_audit");

            migrationBuilder.DropForeignKey(
                name: "FK_session_key_blobs_coding_sessions_coding_session_id",
                table: "session_key_blobs");

            migrationBuilder.DropPrimaryKey(
                name: "PK_coding_sessions",
                table: "coding_sessions");

            migrationBuilder.DropCheckConstraint(
                name: "CK_coding_sessions_public_scope_is_view_only",
                table: "coding_sessions");

            migrationBuilder.DropCheckConstraint(
                name: "CK_coding_sessions_workspace_scope_matches_workspace_id",
                table: "coding_sessions");

            migrationBuilder.DropPrimaryKey(
                name: "PK_coding_session_access_overrides",
                table: "coding_session_access_overrides");

            migrationBuilder.RenameTable(
                name: "coding_sessions",
                newName: "sessions");

            migrationBuilder.RenameTable(
                name: "coding_session_access_overrides",
                newName: "session_access_overrides");

            migrationBuilder.RenameColumn(
                name: "coding_session_id",
                table: "session_key_blobs",
                newName: "session_id");

            migrationBuilder.RenameIndex(
                name: "IX_session_key_blobs_coding_session_id",
                table: "session_key_blobs",
                newName: "IX_session_key_blobs_session_id");

            migrationBuilder.RenameColumn(
                name: "coding_session_id",
                table: "input_audit",
                newName: "session_id");

            migrationBuilder.RenameIndex(
                name: "IX_input_audit_coding_session_id_sender_user_id_client_command~",
                table: "input_audit",
                newName: "IX_input_audit_session_id_sender_user_id_client_command_id");

            migrationBuilder.RenameIndex(
                name: "IX_input_audit_coding_session_id",
                table: "input_audit",
                newName: "IX_input_audit_session_id");

            migrationBuilder.RenameColumn(
                name: "owner_coding_session_secret_hash",
                table: "sessions",
                newName: "owner_session_secret_hash");

            migrationBuilder.RenameIndex(
                name: "IX_coding_sessions_workspace_id",
                table: "sessions",
                newName: "IX_sessions_workspace_id");

            migrationBuilder.RenameIndex(
                name: "IX_coding_sessions_status",
                table: "sessions",
                newName: "IX_sessions_status");

            migrationBuilder.RenameIndex(
                name: "IX_coding_sessions_scope_status",
                table: "sessions",
                newName: "IX_sessions_scope_status");

            migrationBuilder.RenameIndex(
                name: "IX_coding_sessions_owner_user_id_status_scope",
                table: "sessions",
                newName: "IX_sessions_owner_user_id_status_scope");

            migrationBuilder.RenameIndex(
                name: "IX_coding_sessions_owner_user_id",
                table: "sessions",
                newName: "IX_sessions_owner_user_id");

            migrationBuilder.RenameColumn(
                name: "coding_session_id",
                table: "session_access_overrides",
                newName: "session_id");

            migrationBuilder.RenameIndex(
                name: "IX_coding_session_access_overrides_granted_by_user_id",
                table: "session_access_overrides",
                newName: "IX_session_access_overrides_granted_by_user_id");

            migrationBuilder.RenameIndex(
                name: "IX_coding_session_access_overrides_actor_user_id",
                table: "session_access_overrides",
                newName: "IX_session_access_overrides_actor_user_id");

            migrationBuilder.AddPrimaryKey(
                name: "PK_sessions",
                table: "sessions",
                column: "id");

            migrationBuilder.AddPrimaryKey(
                name: "PK_session_access_overrides",
                table: "session_access_overrides",
                columns: new[] { "session_id", "actor_user_id" });

            migrationBuilder.AddCheckConstraint(
                name: "CK_sessions_public_scope_is_view_only",
                table: "sessions",
                sql: "\"scope\" <> 'Public' OR \"default_access\" = 'View'");

            migrationBuilder.AddCheckConstraint(
                name: "CK_sessions_workspace_scope_matches_workspace_id",
                table: "sessions",
                sql: "(\"scope\" = 'Workspace' AND \"workspace_id\" IS NOT NULL)\nOR (\"scope\" <> 'Workspace' AND \"workspace_id\" IS NULL)");

            migrationBuilder.AddForeignKey(
                name: "FK_input_audit_sessions_session_id",
                table: "input_audit",
                column: "session_id",
                principalTable: "sessions",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_session_access_overrides_sessions_session_id",
                table: "session_access_overrides",
                column: "session_id",
                principalTable: "sessions",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_session_access_overrides_users_actor_user_id",
                table: "session_access_overrides",
                column: "actor_user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_session_access_overrides_users_granted_by_user_id",
                table: "session_access_overrides",
                column: "granted_by_user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_session_key_blobs_sessions_session_id",
                table: "session_key_blobs",
                column: "session_id",
                principalTable: "sessions",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_sessions_users_owner_user_id",
                table: "sessions",
                column: "owner_user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_sessions_workspaces_workspace_id",
                table: "sessions",
                column: "workspace_id",
                principalTable: "workspaces",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropForeignKey(
                name: "FK_input_audit_sessions_session_id",
                table: "input_audit");

            migrationBuilder.DropForeignKey(
                name: "FK_session_access_overrides_sessions_session_id",
                table: "session_access_overrides");

            migrationBuilder.DropForeignKey(
                name: "FK_session_access_overrides_users_actor_user_id",
                table: "session_access_overrides");

            migrationBuilder.DropForeignKey(
                name: "FK_session_access_overrides_users_granted_by_user_id",
                table: "session_access_overrides");

            migrationBuilder.DropForeignKey(
                name: "FK_session_key_blobs_sessions_session_id",
                table: "session_key_blobs");

            migrationBuilder.DropForeignKey(
                name: "FK_sessions_users_owner_user_id",
                table: "sessions");

            migrationBuilder.DropForeignKey(
                name: "FK_sessions_workspaces_workspace_id",
                table: "sessions");

            migrationBuilder.DropPrimaryKey(
                name: "PK_sessions",
                table: "sessions");

            migrationBuilder.DropCheckConstraint(
                name: "CK_sessions_public_scope_is_view_only",
                table: "sessions");

            migrationBuilder.DropCheckConstraint(
                name: "CK_sessions_workspace_scope_matches_workspace_id",
                table: "sessions");

            migrationBuilder.DropPrimaryKey(
                name: "PK_session_access_overrides",
                table: "session_access_overrides");

            migrationBuilder.RenameTable(
                name: "sessions",
                newName: "coding_sessions");

            migrationBuilder.RenameTable(
                name: "session_access_overrides",
                newName: "coding_session_access_overrides");

            migrationBuilder.RenameColumn(
                name: "session_id",
                table: "session_key_blobs",
                newName: "coding_session_id");

            migrationBuilder.RenameIndex(
                name: "IX_session_key_blobs_session_id",
                table: "session_key_blobs",
                newName: "IX_session_key_blobs_coding_session_id");

            migrationBuilder.RenameColumn(
                name: "session_id",
                table: "input_audit",
                newName: "coding_session_id");

            migrationBuilder.RenameIndex(
                name: "IX_input_audit_session_id_sender_user_id_client_command_id",
                table: "input_audit",
                newName: "IX_input_audit_coding_session_id_sender_user_id_client_command~");

            migrationBuilder.RenameIndex(
                name: "IX_input_audit_session_id",
                table: "input_audit",
                newName: "IX_input_audit_coding_session_id");

            migrationBuilder.RenameColumn(
                name: "owner_session_secret_hash",
                table: "coding_sessions",
                newName: "owner_coding_session_secret_hash");

            migrationBuilder.RenameIndex(
                name: "IX_sessions_workspace_id",
                table: "coding_sessions",
                newName: "IX_coding_sessions_workspace_id");

            migrationBuilder.RenameIndex(
                name: "IX_sessions_status",
                table: "coding_sessions",
                newName: "IX_coding_sessions_status");

            migrationBuilder.RenameIndex(
                name: "IX_sessions_scope_status",
                table: "coding_sessions",
                newName: "IX_coding_sessions_scope_status");

            migrationBuilder.RenameIndex(
                name: "IX_sessions_owner_user_id_status_scope",
                table: "coding_sessions",
                newName: "IX_coding_sessions_owner_user_id_status_scope");

            migrationBuilder.RenameIndex(
                name: "IX_sessions_owner_user_id",
                table: "coding_sessions",
                newName: "IX_coding_sessions_owner_user_id");

            migrationBuilder.RenameColumn(
                name: "session_id",
                table: "coding_session_access_overrides",
                newName: "coding_session_id");

            migrationBuilder.RenameIndex(
                name: "IX_session_access_overrides_granted_by_user_id",
                table: "coding_session_access_overrides",
                newName: "IX_coding_session_access_overrides_granted_by_user_id");

            migrationBuilder.RenameIndex(
                name: "IX_session_access_overrides_actor_user_id",
                table: "coding_session_access_overrides",
                newName: "IX_coding_session_access_overrides_actor_user_id");

            migrationBuilder.AddPrimaryKey(
                name: "PK_coding_sessions",
                table: "coding_sessions",
                column: "id");

            migrationBuilder.AddPrimaryKey(
                name: "PK_coding_session_access_overrides",
                table: "coding_session_access_overrides",
                columns: new[] { "coding_session_id", "actor_user_id" });

            migrationBuilder.AddCheckConstraint(
                name: "CK_coding_sessions_public_scope_is_view_only",
                table: "coding_sessions",
                sql: "\"scope\" <> 'Public' OR \"default_access\" = 'View'");

            migrationBuilder.AddCheckConstraint(
                name: "CK_coding_sessions_workspace_scope_matches_workspace_id",
                table: "coding_sessions",
                sql: "(\"scope\" = 'Workspace' AND \"workspace_id\" IS NOT NULL)\nOR (\"scope\" <> 'Workspace' AND \"workspace_id\" IS NULL)");

            migrationBuilder.AddForeignKey(
                name: "FK_coding_session_access_overrides_coding_sessions_coding_sess~",
                table: "coding_session_access_overrides",
                column: "coding_session_id",
                principalTable: "coding_sessions",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_coding_session_access_overrides_users_actor_user_id",
                table: "coding_session_access_overrides",
                column: "actor_user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_coding_session_access_overrides_users_granted_by_user_id",
                table: "coding_session_access_overrides",
                column: "granted_by_user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_coding_sessions_users_owner_user_id",
                table: "coding_sessions",
                column: "owner_user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_coding_sessions_workspaces_workspace_id",
                table: "coding_sessions",
                column: "workspace_id",
                principalTable: "workspaces",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_input_audit_coding_sessions_coding_session_id",
                table: "input_audit",
                column: "coding_session_id",
                principalTable: "coding_sessions",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_session_key_blobs_coding_sessions_coding_session_id",
                table: "session_key_blobs",
                column: "coding_session_id",
                principalTable: "coding_sessions",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);
        }
    }
}
