using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddSessionEndMutationReceipts : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "session_end_mutations",
                columns: table => new
                {
                    owner_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    mutation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    session_id = table.Column<Guid>(type: "uuid", nullable: false),
                    incarnation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    first_attempt_id = table.Column<Guid>(type: "uuid", nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_session_end_mutations", x => new { x.owner_user_id, x.mutation_id });
                    table.ForeignKey(
                        name: "FK_session_end_mutations_users_owner_user_id",
                        column: x => x.owner_user_id,
                        principalTable: "users",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateIndex(
                name: "IX_session_end_mutations_session_id_incarnation_id",
                table: "session_end_mutations",
                columns: new[] { "session_id", "incarnation_id" });
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM session_end_mutations
                        LIMIT 1
                    ) THEN
                        RAISE EXCEPTION
                            'AddSessionEndMutationReceipts rollback cannot discard committed teardown idempotency history'
                            USING HINT = 'Keep this migration applied; receipt retention is required to make retried session teardown safe.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.DropTable(
                name: "session_end_mutations");
        }
    }
}
