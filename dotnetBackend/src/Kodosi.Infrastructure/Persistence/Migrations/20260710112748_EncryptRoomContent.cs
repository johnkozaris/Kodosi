using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class EncryptRoomContent : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {


            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (SELECT 1 FROM room_chat_messages LIMIT 1)
                       OR EXISTS (SELECT 1 FROM room_tasks LIMIT 1) THEN
                        RAISE EXCEPTION
                            'EncryptRoomContent requires empty legacy room_chat_messages and room_tasks tables'
                            USING HINT = 'Export legacy room content, complete a client-assisted encryption migration, then retry.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.AlterColumn<string>(
                name: "title",
                table: "room_tasks",
                type: "text",
                nullable: false,
                oldClrType: typeof(string),
                oldType: "character varying(200)",
                oldMaxLength: 200);

            migrationBuilder.AddColumn<Guid>(
                name: "result_author_user_id",
                table: "room_tasks",
                type: "uuid",
                nullable: true);

            migrationBuilder.AlterColumn<string>(
                name: "body",
                table: "room_chat_messages",
                type: "text",
                nullable: false,
                oldClrType: typeof(string),
                oldType: "character varying(4000)",
                oldMaxLength: 4000);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM room_tasks
                        WHERE char_length(title) > 200
                        LIMIT 1
                    ) OR EXISTS (
                        SELECT 1
                        FROM room_chat_messages
                        WHERE char_length(body) > 4000
                        LIMIT 1
                    ) THEN
                        RAISE EXCEPTION
                            'EncryptRoomContent rollback cannot narrow encrypted room content to legacy varchar limits'
                            USING HINT = 'Keep this migration applied or remove oversized encrypted content through an explicit data-retention workflow before retrying rollback.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.DropColumn(
                name: "result_author_user_id",
                table: "room_tasks");

            migrationBuilder.AlterColumn<string>(
                name: "title",
                table: "room_tasks",
                type: "character varying(200)",
                maxLength: 200,
                nullable: false,
                oldClrType: typeof(string),
                oldType: "text");

            migrationBuilder.AlterColumn<string>(
                name: "body",
                table: "room_chat_messages",
                type: "character varying(4000)",
                maxLength: 4000,
                nullable: false,
                oldClrType: typeof(string),
                oldType: "text");
        }
    }
}
