using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class BoundRoomChatPayloads : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddCheckConstraint(
                name: "ck_room_chat_messages_body_length",
                table: "room_chat_messages",
                sql: "char_length(body) BETWEEN 1 AND 1048576");

            migrationBuilder.AddCheckConstraint(
                name: "ck_room_chat_messages_recipient_count",
                table: "room_chat_messages",
                sql: "cardinality(recipient_session_ids) + cardinality(recipient_user_ids) <= 32");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "ck_room_chat_messages_body_length",
                table: "room_chat_messages");

            migrationBuilder.DropCheckConstraint(
                name: "ck_room_chat_messages_recipient_count",
                table: "room_chat_messages");
        }
    }
}
