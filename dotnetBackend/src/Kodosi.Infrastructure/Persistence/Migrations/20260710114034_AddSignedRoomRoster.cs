using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddSignedRoomRoster : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {


            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (SELECT 1 FROM rooms LIMIT 1)
                       OR EXISTS (SELECT 1 FROM room_members LIMIT 1)
                       OR EXISTS (SELECT 1 FROM room_invitations LIMIT 1)
                       OR EXISTS (SELECT 1 FROM room_chat_messages LIMIT 1)
                       OR EXISTS (SELECT 1 FROM room_tasks LIMIT 1)
                       OR EXISTS (SELECT 1 FROM sessions WHERE room_id IS NOT NULL LIMIT 1) THEN
                        RAISE EXCEPTION
                            'AddSignedRoomRoster requires an empty legacy room graph'
                            USING HINT = 'Export the legacy room graph and obtain owner-signed generation-1 rosters before retrying.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.AddColumn<byte[]>(
                name: "roster_body",
                table: "rooms",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);

            migrationBuilder.AddColumn<long>(
                name: "roster_generation",
                table: "rooms",
                type: "bigint",
                nullable: false,
                defaultValue: 0L);

            migrationBuilder.AddColumn<byte[]>(
                name: "roster_signature",
                table: "rooms",
                type: "bytea",
                nullable: false,
                defaultValue: new byte[0]);

            migrationBuilder.AddColumn<string>(
                name: "roster_signer_device_id",
                table: "rooms",
                type: "character varying(256)",
                maxLength: 256,
                nullable: false,
                defaultValue: "");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropColumn(
                name: "roster_body",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_generation",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_signature",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "roster_signer_device_id",
                table: "rooms");
        }
    }
}
