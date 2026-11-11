using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class NameProjects : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            // EF Core scaffolds a drop and a create for a renamed table. These renames keep the rows.
            migrationBuilder.RenameTable(name: "missions", newName: "projects");
            migrationBuilder.RenameTable(name: "mission_invitations", newName: "project_invitations");
            migrationBuilder.RenameTable(name: "mission_items", newName: "project_items");
            migrationBuilder.RenameTable(name: "mission_key_states", newName: "project_key_states");
            migrationBuilder.RenameTable(name: "mission_members", newName: "project_members");
            migrationBuilder.RenameTable(name: "room_recipient_keys", newName: "project_recipient_keys");
            migrationBuilder.RenameColumn(name: "MissionId", table: "project_invitations", newName: "ProjectId");
            migrationBuilder.RenameColumn(name: "MissionId", table: "project_items", newName: "ProjectId");
            migrationBuilder.RenameColumn(name: "MissionId", table: "project_key_states", newName: "ProjectId");
            migrationBuilder.RenameColumn(name: "MissionId", table: "project_members", newName: "ProjectId");
            migrationBuilder.RenameColumn(name: "MissionId", table: "sessions", newName: "ProjectId");
            migrationBuilder.RenameColumn(name: "RoomDeviceId", table: "sessions", newName: "ProjectDeviceId");
            migrationBuilder.RenameColumn(name: "RoomHistory", table: "sessions", newName: "ProjectHistory");
            migrationBuilder.RenameColumn(name: "RoomSequence", table: "sessions", newName: "ProjectSequence");
            migrationBuilder.RenameColumn(name: "RoomSignature", table: "sessions", newName: "ProjectSignature");
            migrationBuilder.Sql("""ALTER TABLE "projects" RENAME CONSTRAINT "PK_missions" TO "PK_projects";""");
            migrationBuilder.Sql("""ALTER TABLE "projects" RENAME CONSTRAINT "FK_missions_users_OwnerUserId" TO "FK_projects_users_OwnerUserId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_invitations" RENAME CONSTRAINT "PK_mission_invitations" TO "PK_project_invitations";""");
            migrationBuilder.Sql("""ALTER TABLE "project_invitations" RENAME CONSTRAINT "FK_mission_invitations_missions_MissionId" TO "FK_project_invitations_projects_ProjectId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_invitations" RENAME CONSTRAINT "FK_mission_invitations_users_UserId" TO "FK_project_invitations_users_UserId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_items" RENAME CONSTRAINT "PK_mission_items" TO "PK_project_items";""");
            migrationBuilder.Sql("""ALTER TABLE "project_items" RENAME CONSTRAINT "FK_mission_items_missions_MissionId" TO "FK_project_items_projects_ProjectId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_key_states" RENAME CONSTRAINT "PK_mission_key_states" TO "PK_project_key_states";""");
            migrationBuilder.Sql("""ALTER TABLE "project_key_states" RENAME CONSTRAINT "FK_mission_key_states_missions_MissionId" TO "FK_project_key_states_projects_ProjectId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_members" RENAME CONSTRAINT "PK_mission_members" TO "PK_project_members";""");
            migrationBuilder.Sql("""ALTER TABLE "project_members" RENAME CONSTRAINT "FK_mission_members_missions_MissionId" TO "FK_project_members_projects_ProjectId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_members" RENAME CONSTRAINT "FK_mission_members_users_UserId" TO "FK_project_members_users_UserId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_recipient_keys" RENAME CONSTRAINT "PK_room_recipient_keys" TO "PK_project_recipient_keys";""");
            migrationBuilder.Sql("""ALTER TABLE "project_recipient_keys" RENAME CONSTRAINT "FK_room_recipient_keys_devices_DeviceId" TO "FK_project_recipient_keys_devices_DeviceId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_recipient_keys" RENAME CONSTRAINT "FK_room_recipient_keys_users_UserId" TO "FK_project_recipient_keys_users_UserId";""");
            migrationBuilder.Sql("""ALTER TABLE "sessions" RENAME CONSTRAINT "FK_sessions_missions_MissionId" TO "FK_sessions_projects_ProjectId";""");
            migrationBuilder.RenameIndex(name: "IX_missions_Dosi_OwnerUserId", table: "projects", newName: "IX_projects_Dosi_OwnerUserId");
            migrationBuilder.RenameIndex(name: "IX_missions_OwnerUserId", table: "projects", newName: "IX_projects_OwnerUserId");
            migrationBuilder.RenameIndex(name: "IX_mission_invitations_MissionId_UserId", table: "project_invitations", newName: "IX_project_invitations_ProjectId_UserId");
            migrationBuilder.RenameIndex(name: "IX_mission_invitations_UserId", table: "project_invitations", newName: "IX_project_invitations_UserId");
            migrationBuilder.RenameIndex(name: "IX_mission_items_MissionId_Kind_Sequence", table: "project_items", newName: "IX_project_items_ProjectId_Kind_Sequence");
            migrationBuilder.RenameIndex(name: "IX_mission_items_MissionId_Sequence", table: "project_items", newName: "IX_project_items_ProjectId_Sequence");
            migrationBuilder.RenameIndex(name: "IX_mission_members_UserId", table: "project_members", newName: "IX_project_members_UserId");
            migrationBuilder.RenameIndex(name: "IX_room_recipient_keys_UserId", table: "project_recipient_keys", newName: "IX_project_recipient_keys_UserId");
            migrationBuilder.RenameIndex(name: "IX_sessions_MissionId", table: "sessions", newName: "IX_sessions_ProjectId");
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.RenameColumn(name: "ProjectId", table: "project_invitations", newName: "MissionId");
            migrationBuilder.RenameColumn(name: "ProjectId", table: "project_items", newName: "MissionId");
            migrationBuilder.RenameColumn(name: "ProjectId", table: "project_key_states", newName: "MissionId");
            migrationBuilder.RenameColumn(name: "ProjectId", table: "project_members", newName: "MissionId");
            migrationBuilder.RenameColumn(name: "ProjectId", table: "sessions", newName: "MissionId");
            migrationBuilder.RenameColumn(name: "ProjectDeviceId", table: "sessions", newName: "RoomDeviceId");
            migrationBuilder.RenameColumn(name: "ProjectHistory", table: "sessions", newName: "RoomHistory");
            migrationBuilder.RenameColumn(name: "ProjectSequence", table: "sessions", newName: "RoomSequence");
            migrationBuilder.RenameColumn(name: "ProjectSignature", table: "sessions", newName: "RoomSignature");
            migrationBuilder.Sql("""ALTER TABLE "projects" RENAME CONSTRAINT "PK_projects" TO "PK_missions";""");
            migrationBuilder.Sql("""ALTER TABLE "projects" RENAME CONSTRAINT "FK_projects_users_OwnerUserId" TO "FK_missions_users_OwnerUserId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_invitations" RENAME CONSTRAINT "PK_project_invitations" TO "PK_mission_invitations";""");
            migrationBuilder.Sql("""ALTER TABLE "project_invitations" RENAME CONSTRAINT "FK_project_invitations_projects_ProjectId" TO "FK_mission_invitations_missions_MissionId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_invitations" RENAME CONSTRAINT "FK_project_invitations_users_UserId" TO "FK_mission_invitations_users_UserId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_items" RENAME CONSTRAINT "PK_project_items" TO "PK_mission_items";""");
            migrationBuilder.Sql("""ALTER TABLE "project_items" RENAME CONSTRAINT "FK_project_items_projects_ProjectId" TO "FK_mission_items_missions_MissionId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_key_states" RENAME CONSTRAINT "PK_project_key_states" TO "PK_mission_key_states";""");
            migrationBuilder.Sql("""ALTER TABLE "project_key_states" RENAME CONSTRAINT "FK_project_key_states_projects_ProjectId" TO "FK_mission_key_states_missions_MissionId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_members" RENAME CONSTRAINT "PK_project_members" TO "PK_mission_members";""");
            migrationBuilder.Sql("""ALTER TABLE "project_members" RENAME CONSTRAINT "FK_project_members_projects_ProjectId" TO "FK_mission_members_missions_MissionId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_members" RENAME CONSTRAINT "FK_project_members_users_UserId" TO "FK_mission_members_users_UserId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_recipient_keys" RENAME CONSTRAINT "PK_project_recipient_keys" TO "PK_room_recipient_keys";""");
            migrationBuilder.Sql("""ALTER TABLE "project_recipient_keys" RENAME CONSTRAINT "FK_project_recipient_keys_devices_DeviceId" TO "FK_room_recipient_keys_devices_DeviceId";""");
            migrationBuilder.Sql("""ALTER TABLE "project_recipient_keys" RENAME CONSTRAINT "FK_project_recipient_keys_users_UserId" TO "FK_room_recipient_keys_users_UserId";""");
            migrationBuilder.Sql("""ALTER TABLE "sessions" RENAME CONSTRAINT "FK_sessions_projects_ProjectId" TO "FK_sessions_missions_MissionId";""");
            migrationBuilder.RenameIndex(name: "IX_projects_Dosi_OwnerUserId", table: "projects", newName: "IX_missions_Dosi_OwnerUserId");
            migrationBuilder.RenameIndex(name: "IX_projects_OwnerUserId", table: "projects", newName: "IX_missions_OwnerUserId");
            migrationBuilder.RenameIndex(name: "IX_project_invitations_ProjectId_UserId", table: "project_invitations", newName: "IX_mission_invitations_MissionId_UserId");
            migrationBuilder.RenameIndex(name: "IX_project_invitations_UserId", table: "project_invitations", newName: "IX_mission_invitations_UserId");
            migrationBuilder.RenameIndex(name: "IX_project_items_ProjectId_Kind_Sequence", table: "project_items", newName: "IX_mission_items_MissionId_Kind_Sequence");
            migrationBuilder.RenameIndex(name: "IX_project_items_ProjectId_Sequence", table: "project_items", newName: "IX_mission_items_MissionId_Sequence");
            migrationBuilder.RenameIndex(name: "IX_project_members_UserId", table: "project_members", newName: "IX_mission_members_UserId");
            migrationBuilder.RenameIndex(name: "IX_project_recipient_keys_UserId", table: "project_recipient_keys", newName: "IX_room_recipient_keys_UserId");
            migrationBuilder.RenameIndex(name: "IX_sessions_ProjectId", table: "sessions", newName: "IX_sessions_MissionId");
            migrationBuilder.RenameTable(name: "project_recipient_keys", newName: "room_recipient_keys");
            migrationBuilder.RenameTable(name: "project_members", newName: "mission_members");
            migrationBuilder.RenameTable(name: "project_key_states", newName: "mission_key_states");
            migrationBuilder.RenameTable(name: "project_items", newName: "mission_items");
            migrationBuilder.RenameTable(name: "project_invitations", newName: "mission_invitations");
            migrationBuilder.RenameTable(name: "projects", newName: "missions");
        }
    }
}
