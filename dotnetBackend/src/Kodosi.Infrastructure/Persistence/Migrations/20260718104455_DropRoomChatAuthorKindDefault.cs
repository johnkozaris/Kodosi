using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class DropRoomChatAuthorKindDefault : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                ALTER TABLE room_chat_messages
                    ALTER COLUMN author_kind DROP DEFAULT;
                """);
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
                        WHERE author_kind = 'Agent'
                        LIMIT 1
                    ) THEN
                        RAISE EXCEPTION
                            'DropRoomChatAuthorKindDefault rollback cannot discard Agent attribution'
                            USING HINT = 'Keep this migration applied or remove Agent-attributed messages through an explicit data-retention workflow before retrying rollback.';
                    END IF;
                END
                $kodosi$;

                ALTER TABLE room_chat_messages
                    ALTER COLUMN author_kind SET DEFAULT 'Human';
                """);
        }
    }
}
