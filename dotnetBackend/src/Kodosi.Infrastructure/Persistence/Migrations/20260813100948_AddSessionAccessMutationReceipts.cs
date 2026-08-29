using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddSessionAccessMutationReceipts : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "session_access_mutations",
                columns: table => new
                {
                    requester_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    mutation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    session_id = table.Column<Guid>(type: "uuid", nullable: false),
                    incarnation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    kind = table.Column<string>(type: "character varying(16)", maxLength: 16, nullable: false),
                    target_user_id = table.Column<Guid>(type: "uuid", nullable: true),
                    access_level = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: true),
                    requested_expires_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_session_access_mutations", x => new { x.requester_user_id, x.mutation_id });
                    table.CheckConstraint("CK_session_access_mutations_access_level", "access_level IS NULL OR access_level IN ('View', 'Suggest', 'Inject', 'Approve')");
                    table.CheckConstraint("CK_session_access_mutations_kind", "kind IN ('Grant', 'Revoke', 'Leave')");
                    table.CheckConstraint("CK_session_access_mutations_shape", "(kind = 'Grant' AND target_user_id IS NOT NULL AND access_level IS NOT NULL AND requested_expires_at IS NOT NULL) OR (kind = 'Revoke' AND target_user_id IS NOT NULL AND access_level IS NULL AND requested_expires_at IS NULL) OR (kind = 'Leave' AND target_user_id IS NULL AND access_level IS NULL AND requested_expires_at IS NULL)");
                    table.ForeignKey(
                        name: "FK_session_access_mutations_users_requester_user_id",
                        column: x => x.requester_user_id,
                        principalTable: "users",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateIndex(
                name: "IX_session_access_mutations_session_id_incarnation_id",
                table: "session_access_mutations",
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
                        FROM session_access_mutations
                        LIMIT 1
                    ) THEN
                        RAISE EXCEPTION
                            'AddSessionAccessMutationReceipts rollback cannot discard committed access mutation idempotency history'
                            USING HINT = 'Keep this migration applied; receipt retention is required to make retried grant, revoke, and leave mutations safe.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.DropTable(
                name: "session_access_mutations");
        }
    }
}
