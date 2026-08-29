using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddSessionIncarnationId : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<Guid>(
                name: "incarnation_id",
                table: "sessions",
                type: "uuid",
                nullable: true);
            migrationBuilder.AddColumn<long>(
                name: "incarnation_generation",
                table: "sessions",
                type: "bigint",
                nullable: true);
            migrationBuilder.AddColumn<int>(
                name: "incarnation_protocol_version",
                table: "sessions",
                type: "integer",
                nullable: true);
            migrationBuilder.AddColumn<int>(
                name: "signature_version",
                table: "session_key_blobs",
                type: "integer",
                nullable: false,
                defaultValue: 1);

            migrationBuilder.Sql(
                """
                UPDATE sessions
                SET incarnation_id = gen_random_uuid(),
                    incarnation_generation = 1,
                    incarnation_protocol_version = 1
                """);

            migrationBuilder.Sql(
                """
                DELETE FROM session_viewer_dismissals AS dismissal
                USING sessions AS session
                WHERE dismissal.session_id = session.id
                  AND dismissal.created_at < session.started_at
                """);

            migrationBuilder.AlterColumn<Guid>(
                name: "incarnation_id",
                table: "sessions",
                type: "uuid",
                nullable: false,
                oldClrType: typeof(Guid),
                oldType: "uuid",
                oldNullable: true);
            migrationBuilder.AlterColumn<long>(
                name: "incarnation_generation",
                table: "sessions",
                type: "bigint",
                nullable: false,
                oldClrType: typeof(long),
                oldType: "bigint",
                oldNullable: true);
            migrationBuilder.AlterColumn<int>(
                name: "incarnation_protocol_version",
                table: "sessions",
                type: "integer",
                nullable: false,
                oldClrType: typeof(int),
                oldType: "integer",
                oldNullable: true);

            migrationBuilder.CreateTable(
                name: "session_incarnations",
                columns: table => new
                {
                    session_id = table.Column<Guid>(type: "uuid", nullable: false),
                    generation = table.Column<long>(type: "bigint", nullable: false),
                    incarnation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    protocol_version = table.Column<int>(type: "integer", nullable: false),
                    idempotency_key = table.Column<Guid>(type: "uuid", nullable: false),
                    created_at = table.Column<DateTimeOffset>(
                        type: "timestamp with time zone",
                        nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey(
                        "PK_session_incarnations",
                        x => new { x.session_id, x.generation });
                    table.ForeignKey(
                        name: "FK_session_incarnations_sessions_session_id",
                        column: x => x.session_id,
                        principalTable: "sessions",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.Sql(
                """
                INSERT INTO session_incarnations (
                    session_id,
                    generation,
                    incarnation_id,
                    protocol_version,
                    idempotency_key,
                    created_at
                )
                SELECT
                    id,
                    incarnation_generation,
                    incarnation_id,
                    incarnation_protocol_version,
                    incarnation_id,
                    started_at
                FROM sessions
                """);

            migrationBuilder.CreateIndex(
                name: "UX_session_incarnations_idempotency",
                table: "session_incarnations",
                columns: new[] { "session_id", "idempotency_key" },
                unique: true);
            migrationBuilder.CreateIndex(
                name: "UX_session_incarnations_incarnation_id",
                table: "session_incarnations",
                column: "incarnation_id",
                unique: true);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "session_incarnations");
            migrationBuilder.DropColumn(
                name: "signature_version",
                table: "session_key_blobs");
            migrationBuilder.DropColumn(
                name: "incarnation_generation",
                table: "sessions");
            migrationBuilder.DropColumn(
                name: "incarnation_protocol_version",
                table: "sessions");
            migrationBuilder.DropColumn(
                name: "incarnation_id",
                table: "sessions");
        }
    }
}
