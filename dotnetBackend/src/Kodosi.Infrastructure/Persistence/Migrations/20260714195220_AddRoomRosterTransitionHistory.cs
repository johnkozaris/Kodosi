using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddRoomRosterTransitionHistory : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "room_roster_transitions",
                columns: table => new
                {
                    room_id = table.Column<Guid>(type: "uuid", nullable: false),
                    generation = table.Column<long>(type: "bigint", nullable: false),
                    roster_body = table.Column<byte[]>(type: "bytea", nullable: false),
                    roster_signature = table.Column<byte[]>(type: "bytea", nullable: false),
                    roster_signer_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    admission_invitation_id = table.Column<Guid>(type: "uuid", nullable: true),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_room_roster_transitions", x => new { x.room_id, x.generation });
                    table.ForeignKey(
                        name: "FK_room_roster_transitions_room_invitations_admission_invitati~",
                        column: x => x.admission_invitation_id,
                        principalTable: "room_invitations",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "FK_room_roster_transitions_rooms_room_id",
                        column: x => x.room_id,
                        principalTable: "rooms",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateIndex(
                name: "IX_room_roster_transitions_admission_invitation_id",
                table: "room_roster_transitions",
                column: "admission_invitation_id");



            migrationBuilder.Sql("""
                INSERT INTO room_roster_transitions
                    (room_id, generation, roster_body, roster_signature,
                     roster_signer_device_id, admission_invitation_id, created_at)
                SELECT id, roster_generation, roster_body, roster_signature,
                       roster_signer_device_id, roster_activation_invitation_id, created_at
                FROM rooms
                ON CONFLICT (room_id, generation) DO NOTHING
                """);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "room_roster_transitions");
        }
    }
}
