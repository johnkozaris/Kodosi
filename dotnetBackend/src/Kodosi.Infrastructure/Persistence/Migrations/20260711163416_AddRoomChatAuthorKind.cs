using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddRoomChatAuthorKind : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<string>(
                name: "author_kind",
                table: "room_chat_messages",
                type: "character varying(16)",
                maxLength: 16,
                nullable: false,
                defaultValue: "Human");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropColumn(
                name: "author_kind",
                table: "room_chat_messages");
        }
    }
}
