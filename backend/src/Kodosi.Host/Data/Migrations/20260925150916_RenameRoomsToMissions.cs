using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations;

public partial class RenameRoomsToMissions : Migration
{
    protected override void Up(MigrationBuilder migrationBuilder)
    {
        migrationBuilder.DropForeignKey(
            name: "FK_sessions_rooms_RoomId",
            table: "sessions");

        migrationBuilder.RenameTable(
            name: "rooms",
            newName: "missions");

        migrationBuilder.RenameTable(
            name: "room_members",
            newName: "mission_members");

        migrationBuilder.RenameTable(
            name: "room_invitations",
            newName: "mission_invitations");

        migrationBuilder.RenameColumn(
            name: "RoomId",
            table: "sessions",
            newName: "MissionId");

        migrationBuilder.RenameColumn(
            name: "RoomId",
            table: "mission_members",
            newName: "MissionId");

        migrationBuilder.RenameColumn(
            name: "RoomId",
            table: "mission_invitations",
            newName: "MissionId");

        migrationBuilder.RenameIndex(
            name: "IX_sessions_RoomId",
            table: "sessions",
            newName: "IX_sessions_MissionId");

        migrationBuilder.RenameIndex(
            name: "IX_rooms_OwnerUserId",
            table: "missions",
            newName: "IX_missions_OwnerUserId");

        migrationBuilder.RenameIndex(
            name: "IX_room_members_UserId",
            table: "mission_members",
            newName: "IX_mission_members_UserId");

        migrationBuilder.RenameIndex(
            name: "IX_room_invitations_RoomId_UserId",
            table: "mission_invitations",
            newName: "IX_mission_invitations_MissionId_UserId");

        migrationBuilder.RenameIndex(
            name: "IX_room_invitations_UserId",
            table: "mission_invitations",
            newName: "IX_mission_invitations_UserId");

        migrationBuilder.Sql(
            """
            ALTER TABLE missions RENAME CONSTRAINT "PK_rooms" TO "PK_missions";
            ALTER TABLE missions RENAME CONSTRAINT "FK_rooms_users_OwnerUserId" TO "FK_missions_users_OwnerUserId";
            ALTER TABLE mission_members RENAME CONSTRAINT "PK_room_members" TO "PK_mission_members";
            ALTER TABLE mission_members RENAME CONSTRAINT "FK_room_members_rooms_RoomId" TO "FK_mission_members_missions_MissionId";
            ALTER TABLE mission_members RENAME CONSTRAINT "FK_room_members_users_UserId" TO "FK_mission_members_users_UserId";
            ALTER TABLE mission_invitations RENAME CONSTRAINT "PK_room_invitations" TO "PK_mission_invitations";
            ALTER TABLE mission_invitations RENAME CONSTRAINT "FK_room_invitations_rooms_RoomId" TO "FK_mission_invitations_missions_MissionId";
            ALTER TABLE mission_invitations RENAME CONSTRAINT "FK_room_invitations_users_UserId" TO "FK_mission_invitations_users_UserId";
            """);

        migrationBuilder.AddForeignKey(
            name: "FK_sessions_missions_MissionId",
            table: "sessions",
            column: "MissionId",
            principalTable: "missions",
            principalColumn: "Id",
            onDelete: ReferentialAction.SetNull);
    }

    protected override void Down(MigrationBuilder migrationBuilder)
    {
        migrationBuilder.Sql(
            """
            DO $$ BEGIN
                IF EXISTS (SELECT 1 FROM missions WHERE "Slug" IS NULL)
                   OR EXISTS (SELECT "Slug" FROM missions GROUP BY "Slug" HAVING COUNT(*) > 1)
                THEN RAISE EXCEPTION 'Cannot restore Mission slug constraints without an explicit data migration.';
                END IF;
            END $$;
            """);

        migrationBuilder.DropForeignKey(
            name: "FK_sessions_missions_MissionId",
            table: "sessions");

        migrationBuilder.Sql(
            """
            ALTER TABLE missions RENAME CONSTRAINT "PK_missions" TO "PK_rooms";
            ALTER TABLE missions RENAME CONSTRAINT "FK_missions_users_OwnerUserId" TO "FK_rooms_users_OwnerUserId";
            ALTER TABLE mission_members RENAME CONSTRAINT "PK_mission_members" TO "PK_room_members";
            ALTER TABLE mission_members RENAME CONSTRAINT "FK_mission_members_missions_MissionId" TO "FK_room_members_rooms_RoomId";
            ALTER TABLE mission_members RENAME CONSTRAINT "FK_mission_members_users_UserId" TO "FK_room_members_users_UserId";
            ALTER TABLE mission_invitations RENAME CONSTRAINT "PK_mission_invitations" TO "PK_room_invitations";
            ALTER TABLE mission_invitations RENAME CONSTRAINT "FK_mission_invitations_missions_MissionId" TO "FK_room_invitations_rooms_RoomId";
            ALTER TABLE mission_invitations RENAME CONSTRAINT "FK_mission_invitations_users_UserId" TO "FK_room_invitations_users_UserId";
            """);

        migrationBuilder.RenameIndex(
            name: "IX_sessions_MissionId",
            table: "sessions",
            newName: "IX_sessions_RoomId");

        migrationBuilder.RenameIndex(
            name: "IX_missions_OwnerUserId",
            table: "missions",
            newName: "IX_rooms_OwnerUserId");

        migrationBuilder.RenameIndex(
            name: "IX_mission_members_UserId",
            table: "mission_members",
            newName: "IX_room_members_UserId");

        migrationBuilder.RenameIndex(
            name: "IX_mission_invitations_MissionId_UserId",
            table: "mission_invitations",
            newName: "IX_room_invitations_RoomId_UserId");

        migrationBuilder.RenameIndex(
            name: "IX_mission_invitations_UserId",
            table: "mission_invitations",
            newName: "IX_room_invitations_UserId");

        migrationBuilder.RenameColumn(
            name: "MissionId",
            table: "sessions",
            newName: "RoomId");

        migrationBuilder.RenameColumn(
            name: "MissionId",
            table: "mission_members",
            newName: "RoomId");

        migrationBuilder.RenameColumn(
            name: "MissionId",
            table: "mission_invitations",
            newName: "RoomId");

        migrationBuilder.RenameTable(
            name: "missions",
            newName: "rooms");

        migrationBuilder.RenameTable(
            name: "mission_members",
            newName: "room_members");

        migrationBuilder.RenameTable(
            name: "mission_invitations",
            newName: "room_invitations");

        migrationBuilder.AddForeignKey(
            name: "FK_sessions_rooms_RoomId",
            table: "sessions",
            column: "RoomId",
            principalTable: "rooms",
            principalColumn: "Id",
            onDelete: ReferentialAction.SetNull);
    }
}
