using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddRoomMutationReceipts : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<Guid>(
                name: "assigned_session_incarnation_id",
                table: "room_tasks",
                type: "uuid",
                nullable: true);

            migrationBuilder.AddColumn<long>(
                name: "revision",
                table: "room_tasks",
                type: "bigint",
                nullable: false,
                defaultValue: 0L);

            migrationBuilder.CreateTable(
                name: "room_mutation_receipts",
                columns: table => new
                {
                    actor_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    operation = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    request_id = table.Column<Guid>(type: "uuid", nullable: false),
                    target_fingerprint = table.Column<byte[]>(type: "bytea", maxLength: 32, nullable: false),
                    room_id = table.Column<Guid>(type: "uuid", nullable: false),
                    entity_id = table.Column<Guid>(type: "uuid", nullable: false),
                    result = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    assignee_session_id = table.Column<Guid>(type: "uuid", nullable: true),
                    assignee_session_incarnation_id = table.Column<Guid>(type: "uuid", nullable: true),
                    revision = table.Column<long>(type: "bigint", nullable: true),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_room_mutation_receipts", x => new { x.actor_user_id, x.operation, x.request_id });
                    table.CheckConstraint("CK_room_mutation_receipts_assignee_pair", "(assignee_session_id IS NULL) = (assignee_session_incarnation_id IS NULL)");
                    table.CheckConstraint("CK_room_mutation_receipts_fingerprint_length", "octet_length(target_fingerprint) = 32");
                    table.CheckConstraint("CK_room_mutation_receipts_operation_allowed_values", "\"operation\" IN ('AcceptInvitation', 'DeclineInvitation', 'CancelInvitation', 'RemoveMember', 'AssignTask', 'TransitionTask')");
                    table.CheckConstraint("CK_room_mutation_receipts_revision_nonnegative", "revision IS NULL OR revision >= 0");
                    table.ForeignKey(
                        name: "FK_room_mutation_receipts_rooms_room_id",
                        column: x => x.room_id,
                        principalTable: "rooms",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "FK_room_mutation_receipts_users_actor_user_id",
                        column: x => x.actor_user_id,
                        principalTable: "users",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateTable(
                name: "room_mutation_session_effects",
                columns: table => new
                {
                    actor_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    operation = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    request_id = table.Column<Guid>(type: "uuid", nullable: false),
                    session_id = table.Column<Guid>(type: "uuid", nullable: false),
                    session_incarnation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    owner_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    started_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    ended_by_removal = table.Column<bool>(type: "boolean", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_room_mutation_session_effects", x => new { x.actor_user_id, x.operation, x.request_id, x.session_id, x.session_incarnation_id });
                    table.CheckConstraint("CK_room_mutation_session_effects_remove_member_only", "operation = 'RemoveMember'");
                    table.ForeignKey(
                        name: "FK_room_mutation_session_effects_room_mutation_receipts_actor_~",
                        columns: x => new { x.actor_user_id, x.operation, x.request_id },
                        principalTable: "room_mutation_receipts",
                        principalColumns: new[] { "actor_user_id", "operation", "request_id" },
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.Sql(
                """
                UPDATE room_tasks AS task
                SET assigned_session_incarnation_id = session.incarnation_id
                FROM sessions AS session
                WHERE task.assigned_session_id = session.id
                  AND task.assigned_session_incarnation_id IS NULL;
                """);

            migrationBuilder.AddCheckConstraint(
                name: "CK_room_tasks_assignee_incarnation_pair",
                table: "room_tasks",
                sql: "(assigned_session_id IS NULL) = (assigned_session_incarnation_id IS NULL)");

            migrationBuilder.CreateIndex(
                name: "IX_room_mutation_receipts_room_id_created_at",
                table: "room_mutation_receipts",
                columns: new[] { "room_id", "created_at" });
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $EF$
                BEGIN
                    IF EXISTS (SELECT 1 FROM room_mutation_receipts LIMIT 1) THEN
                        RAISE EXCEPTION 'Cannot roll back room mutation receipts while idempotency history exists.';
                    END IF;
                END $EF$;
                """);

            migrationBuilder.DropTable(
                name: "room_mutation_session_effects");

            migrationBuilder.DropTable(
                name: "room_mutation_receipts");

            migrationBuilder.DropCheckConstraint(
                name: "CK_room_tasks_assignee_incarnation_pair",
                table: "room_tasks");

            migrationBuilder.DropColumn(
                name: "assigned_session_incarnation_id",
                table: "room_tasks");

            migrationBuilder.DropColumn(
                name: "revision",
                table: "room_tasks");
        }
    }
}
