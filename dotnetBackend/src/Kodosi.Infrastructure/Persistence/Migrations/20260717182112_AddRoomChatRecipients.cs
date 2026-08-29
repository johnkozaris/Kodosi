using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddRoomChatRecipients : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<Guid[]>(
                name: "recipient_session_ids",
                table: "room_chat_messages",
                type: "uuid[]",
                nullable: false,
                defaultValueSql: "'{}'::uuid[]");

            migrationBuilder.AddColumn<Guid[]>(
                name: "recipient_user_ids",
                table: "room_chat_messages",
                type: "uuid[]",
                nullable: false,
                defaultValueSql: "'{}'::uuid[]");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM room_chat_messages
                        WHERE cardinality(recipient_session_ids) > 0
                           OR cardinality(recipient_user_ids) > 0
                        LIMIT 1
                    ) THEN
                        RAISE EXCEPTION
                            'AddRoomChatRecipients rollback cannot discard targeted recipient routing'
                            USING HINT = 'Keep this migration applied or remove targeted messages through an explicit data-retention workflow before retrying rollback.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.DropColumn(
                name: "recipient_session_ids",
                table: "room_chat_messages");

            migrationBuilder.DropColumn(
                name: "recipient_user_ids",
                table: "room_chat_messages");
        }
    }
}
