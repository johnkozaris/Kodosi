using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class ImproveAuditDurabilityAndIntegrity : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                WITH ranked AS (
                    SELECT
                        id,
                        ROW_NUMBER() OVER (
                            PARTITION BY coding_session_id, sender_user_id, client_command_id
                            ORDER BY
                                CASE status
                                    WHEN 'Completed' THEN 6
                                    WHEN 'Accepted' THEN 5
                                    WHEN 'Dispatched' THEN 4
                                    WHEN 'Failed' THEN 3
                                    WHEN 'Rejected' THEN 2
                                    WHEN 'Pending' THEN 1
                                    ELSE 0
                                END DESC,
                                COALESCE(completed_at, dispatched_at, submitted_at) DESC,
                                submitted_at DESC,
                                id DESC
                        ) AS row_number
                    FROM input_audit
                )
                DELETE FROM input_audit
                WHERE id IN (
                    SELECT id
                    FROM ranked
                    WHERE row_number > 1
                );
                """);

            migrationBuilder.CreateIndex(
                name: "IX_workspaces_owner_user_id",
                table: "workspaces",
                column: "owner_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_input_audit_coding_session_id_sender_user_id_client_command~",
                table: "input_audit",
                columns: new[] { "coding_session_id", "sender_user_id", "client_command_id" },
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_input_audit_sender_user_id",
                table: "input_audit",
                column: "sender_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_friendships_requestor_user_id",
                table: "friendships",
                column: "requestor_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_friendships_user_high_id",
                table: "friendships",
                column: "user_high_id");

            migrationBuilder.CreateIndex(
                name: "IX_coding_sessions_workspace_id",
                table: "coding_sessions",
                column: "workspace_id");

            migrationBuilder.CreateIndex(
                name: "IX_coding_session_access_overrides_actor_user_id",
                table: "coding_session_access_overrides",
                column: "actor_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_coding_session_access_overrides_granted_by_user_id",
                table: "coding_session_access_overrides",
                column: "granted_by_user_id");

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
                name: "FK_friendships_users_requestor_user_id",
                table: "friendships",
                column: "requestor_user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_friendships_users_user_high_id",
                table: "friendships",
                column: "user_high_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_friendships_users_user_low_id",
                table: "friendships",
                column: "user_low_id",
                principalTable: "users",
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
                name: "FK_input_audit_users_sender_user_id",
                table: "input_audit",
                column: "sender_user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_session_key_blobs_coding_sessions_coding_session_id",
                table: "session_key_blobs",
                column: "coding_session_id",
                principalTable: "coding_sessions",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_user_devices_users_user_id",
                table: "user_devices",
                column: "user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_workspace_members_users_user_id",
                table: "workspace_members",
                column: "user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_workspace_members_workspaces_workspace_id",
                table: "workspace_members",
                column: "workspace_id",
                principalTable: "workspaces",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);

            migrationBuilder.AddForeignKey(
                name: "FK_workspaces_users_owner_user_id",
                table: "workspaces",
                column: "owner_user_id",
                principalTable: "users",
                principalColumn: "id",
                onDelete: ReferentialAction.Restrict);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
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
                name: "FK_friendships_users_requestor_user_id",
                table: "friendships");

            migrationBuilder.DropForeignKey(
                name: "FK_friendships_users_user_high_id",
                table: "friendships");

            migrationBuilder.DropForeignKey(
                name: "FK_friendships_users_user_low_id",
                table: "friendships");

            migrationBuilder.DropForeignKey(
                name: "FK_input_audit_coding_sessions_coding_session_id",
                table: "input_audit");

            migrationBuilder.DropForeignKey(
                name: "FK_input_audit_users_sender_user_id",
                table: "input_audit");

            migrationBuilder.DropForeignKey(
                name: "FK_session_key_blobs_coding_sessions_coding_session_id",
                table: "session_key_blobs");

            migrationBuilder.DropForeignKey(
                name: "FK_user_devices_users_user_id",
                table: "user_devices");

            migrationBuilder.DropForeignKey(
                name: "FK_workspace_members_users_user_id",
                table: "workspace_members");

            migrationBuilder.DropForeignKey(
                name: "FK_workspace_members_workspaces_workspace_id",
                table: "workspace_members");

            migrationBuilder.DropForeignKey(
                name: "FK_workspaces_users_owner_user_id",
                table: "workspaces");

            migrationBuilder.DropIndex(
                name: "IX_workspaces_owner_user_id",
                table: "workspaces");

            migrationBuilder.DropIndex(
                name: "IX_input_audit_coding_session_id_sender_user_id_client_command~",
                table: "input_audit");

            migrationBuilder.DropIndex(
                name: "IX_input_audit_sender_user_id",
                table: "input_audit");

            migrationBuilder.DropIndex(
                name: "IX_friendships_requestor_user_id",
                table: "friendships");

            migrationBuilder.DropIndex(
                name: "IX_friendships_user_high_id",
                table: "friendships");

            migrationBuilder.DropIndex(
                name: "IX_coding_sessions_workspace_id",
                table: "coding_sessions");

            migrationBuilder.DropIndex(
                name: "IX_coding_session_access_overrides_actor_user_id",
                table: "coding_session_access_overrides");

            migrationBuilder.DropIndex(
                name: "IX_coding_session_access_overrides_granted_by_user_id",
                table: "coding_session_access_overrides");
        }
    }
}
