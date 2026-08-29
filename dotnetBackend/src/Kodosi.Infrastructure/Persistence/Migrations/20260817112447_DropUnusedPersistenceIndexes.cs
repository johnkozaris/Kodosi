using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class DropUnusedPersistenceIndexes : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_user_devices_user_id",
                table: "user_devices");

            migrationBuilder.DropIndex(
                name: "IX_session_key_blobs_session_id",
                table: "session_key_blobs");

            migrationBuilder.DropIndex(
                name: "IX_room_member_audit_actor_user_id",
                table: "room_member_audit");

            migrationBuilder.DropIndex(
                name: "IX_room_member_audit_occurred_at",
                table: "room_member_audit");

            migrationBuilder.DropIndex(
                name: "IX_room_member_audit_room_id",
                table: "room_member_audit");

            migrationBuilder.DropIndex(
                name: "IX_room_member_audit_target_user_id",
                table: "room_member_audit");

            migrationBuilder.DropIndex(
                name: "IX_room_chat_messages_posted_at",
                table: "room_chat_messages");

            migrationBuilder.DropIndex(
                name: "IX_input_audit_session_id",
                table: "input_audit");

            migrationBuilder.DropIndex(
                name: "IX_friendship_audit_occurred_at",
                table: "friendship_audit");

            migrationBuilder.DropIndex(
                name: "IX_device_revocation_audit_actor_user_id",
                table: "device_revocation_audit");

            migrationBuilder.DropIndex(
                name: "IX_device_revocation_audit_revoked_device_id",
                table: "device_revocation_audit");

            migrationBuilder.DropIndex(
                name: "IX_access_override_audit_actor_user_id",
                table: "access_override_audit");

            migrationBuilder.DropIndex(
                name: "IX_access_override_audit_grantee_user_id",
                table: "access_override_audit");

            migrationBuilder.DropIndex(
                name: "IX_access_override_audit_occurred_at",
                table: "access_override_audit");

            migrationBuilder.DropIndex(
                name: "IX_access_override_audit_session_id",
                table: "access_override_audit");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateIndex(
                name: "IX_user_devices_user_id",
                table: "user_devices",
                column: "user_id");

            migrationBuilder.CreateIndex(
                name: "IX_session_key_blobs_session_id",
                table: "session_key_blobs",
                column: "session_id");

            migrationBuilder.CreateIndex(
                name: "IX_room_member_audit_actor_user_id",
                table: "room_member_audit",
                column: "actor_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_room_member_audit_occurred_at",
                table: "room_member_audit",
                column: "occurred_at");

            migrationBuilder.CreateIndex(
                name: "IX_room_member_audit_room_id",
                table: "room_member_audit",
                column: "room_id");

            migrationBuilder.CreateIndex(
                name: "IX_room_member_audit_target_user_id",
                table: "room_member_audit",
                column: "target_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_room_chat_messages_posted_at",
                table: "room_chat_messages",
                column: "posted_at");

            migrationBuilder.CreateIndex(
                name: "IX_input_audit_session_id",
                table: "input_audit",
                column: "session_id");

            migrationBuilder.CreateIndex(
                name: "IX_friendship_audit_occurred_at",
                table: "friendship_audit",
                column: "occurred_at");

            migrationBuilder.CreateIndex(
                name: "IX_device_revocation_audit_actor_user_id",
                table: "device_revocation_audit",
                column: "actor_user_id");

            migrationBuilder.CreateIndex(
                name: "IX_device_revocation_audit_revoked_device_id",
                table: "device_revocation_audit",
                column: "revoked_device_id");

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
    }
}
