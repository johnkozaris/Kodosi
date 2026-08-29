using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddSemanticRelayMailbox : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "semantic_relay_requests",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    session_id = table.Column<Guid>(type: "uuid", nullable: false),
                    incarnation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    requester_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    requester_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    request_id = table.Column<Guid>(type: "uuid", nullable: false),
                    mode = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    payload_sha256 = table.Column<string>(type: "character(64)", fixedLength: true, maxLength: 64, nullable: false),
                    state = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    updated_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_semantic_relay_requests", x => x.id);
                    table.CheckConstraint("CK_semantic_relay_requests_mode", "mode IN ('queue', 'steer', 'stopAndSend')");
                    table.CheckConstraint("CK_semantic_relay_requests_payload_sha256", "payload_sha256 ~ '^[0-9a-f]{64}$'");
                    table.CheckConstraint("CK_semantic_relay_requests_state", "state IN ('Pending', 'Dispatched', 'ReceiptStored', 'Acknowledged')");
                });

            migrationBuilder.CreateTable(
                name: "semantic_relay_receipts",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    request_row_id = table.Column<Guid>(type: "uuid", nullable: false),
                    session_id = table.Column<Guid>(type: "uuid", nullable: false),
                    incarnation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    requester_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    requester_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    request_id = table.Column<Guid>(type: "uuid", nullable: false),
                    mode = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    payload_sha256 = table.Column<string>(type: "character(64)", fixedLength: true, maxLength: 64, nullable: false),
                    outcome = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    owner_user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    owner_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    signature = table.Column<string>(type: "text", nullable: false),
                    stored_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    acknowledged_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_semantic_relay_receipts", x => x.id);
                    table.CheckConstraint("CK_semantic_relay_receipts_mode", "mode IN ('queue', 'steer', 'stopAndSend')");
                    table.CheckConstraint("CK_semantic_relay_receipts_outcome", "outcome IN ('injected', 'cancelled', 'deliveryUnknown')");
                    table.CheckConstraint("CK_semantic_relay_receipts_owner_is_requester", "owner_user_id = requester_user_id");
                    table.CheckConstraint("CK_semantic_relay_receipts_payload_sha256", "payload_sha256 ~ '^[0-9a-f]{64}$'");
                    table.ForeignKey(
                        name: "FK_semantic_relay_receipts_semantic_relay_requests_request_row~",
                        column: x => x.request_row_id,
                        principalTable: "semantic_relay_requests",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateIndex(
                name: "IX_semantic_relay_receipts_request_row_id",
                table: "semantic_relay_receipts",
                column: "request_row_id",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_semantic_relay_receipts_requester_user_id_acknowledged_at",
                table: "semantic_relay_receipts",
                columns: new[] { "requester_user_id", "acknowledged_at" });

            migrationBuilder.CreateIndex(
                name: "IX_semantic_relay_receipts_requester_user_id_request_id",
                table: "semantic_relay_receipts",
                columns: new[] { "requester_user_id", "request_id" },
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_semantic_relay_requests_requester_user_id_request_id",
                table: "semantic_relay_requests",
                columns: new[] { "requester_user_id", "request_id" },
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_semantic_relay_requests_session_id_incarnation_id",
                table: "semantic_relay_requests",
                columns: new[] { "session_id", "incarnation_id" });
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $$
                BEGIN
                    IF EXISTS (SELECT 1 FROM semantic_relay_requests)
                       OR EXISTS (SELECT 1 FROM semantic_relay_receipts) THEN
                        RAISE EXCEPTION 'Cannot remove semantic relay mailbox while idempotency or receipt history exists.';
                    END IF;
                END $$;
                """);

            migrationBuilder.DropTable(
                name: "semantic_relay_receipts");

            migrationBuilder.DropTable(
                name: "semantic_relay_requests");
        }
    }
}
