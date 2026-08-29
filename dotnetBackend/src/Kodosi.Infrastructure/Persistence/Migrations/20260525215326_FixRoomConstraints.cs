using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class FixRoomConstraints : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_room_invitations_room_id_invitee_user_id",
                table: "room_invitations");

            migrationBuilder.CreateIndex(
                name: "UX_room_invitations_pending_room_invitee",
                table: "room_invitations",
                columns: new[] { "room_id", "invitee_user_id" },
                unique: true,
                filter: "\"status\" = 'Pending'");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "UX_room_invitations_pending_room_invitee",
                table: "room_invitations");

            migrationBuilder.CreateIndex(
                name: "IX_room_invitations_room_id_invitee_user_id",
                table: "room_invitations",
                columns: new[] { "room_id", "invitee_user_id" });
        }
    }
}
