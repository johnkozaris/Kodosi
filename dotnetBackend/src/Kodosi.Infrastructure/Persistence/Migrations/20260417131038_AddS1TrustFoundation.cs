using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddS1TrustFoundation : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "cert_expires_at",
                table: "user_devices",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "cert_issued_at",
                table: "user_devices",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "cert_signer_device_id",
                table: "user_devices",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "deleted_at",
                table: "user_devices",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "device_certificate",
                table: "user_devices",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "device_certificate_signature",
                table: "user_devices",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "device_label",
                table: "user_devices",
                type: "character varying(128)",
                maxLength: 128,
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "revoked_at",
                table: "user_devices",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "revoked_by_device_id",
                table: "user_devices",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "aad_context",
                table: "session_key_blobs",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<long>(
                name: "issued_at_ms",
                table: "session_key_blobs",
                type: "bigint",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "host_claimed_at",
                table: "coding_sessions",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "host_connection_slot",
                table: "coding_sessions",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<int>(
                name: "live_participant_count",
                table: "coding_sessions",
                type: "integer",
                nullable: false,
                defaultValue: 0);

            migrationBuilder.CreateTable(
                name: "device_link_requests",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    device_code = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    user_code = table.Column<string>(type: "character varying(32)", maxLength: 32, nullable: false),
                    device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    kem_public_key = table.Column<byte[]>(type: "bytea", maxLength: 1184, nullable: false),
                    signing_public_key = table.Column<byte[]>(type: "bytea", maxLength: 1952, nullable: false),
                    expires_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    approved_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    consumed_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    device_certificate = table.Column<byte[]>(type: "bytea", nullable: true),
                    device_certificate_signature = table.Column<byte[]>(type: "bytea", nullable: true),
                    cert_signer_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: true),
                    device_list_generation = table.Column<long>(type: "bigint", nullable: true),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_device_link_requests", x => x.id);
                    table.ForeignKey(
                        name: "FK_device_link_requests_users_user_id",
                        column: x => x.user_id,
                        principalTable: "users",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "device_registration_challenges",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    challenge = table.Column<byte[]>(type: "bytea", nullable: false),
                    expires_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    consumed_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: true),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    xmin = table.Column<uint>(type: "xid", rowVersion: true, nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_device_registration_challenges", x => x.id);
                    table.ForeignKey(
                        name: "FK_device_registration_challenges_users_user_id",
                        column: x => x.user_id,
                        principalTable: "users",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "user_device_lists",
                columns: table => new
                {
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    generation = table.Column<long>(type: "bigint", nullable: false),
                    device_ids = table.Column<string>(type: "jsonb", nullable: false),
                    signer_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    signature = table.Column<byte[]>(type: "bytea", nullable: false),
                    issued_at_ms = table.Column<long>(type: "bigint", nullable: false),
                    expires_at_ms = table.Column<long>(type: "bigint", nullable: true),
                    created_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_user_device_lists", x => new { x.user_id, x.generation });
                    table.CheckConstraint("CK_user_device_lists_device_ids_bounded", "length(\"device_ids\"::text) < 65536");
                    table.CheckConstraint("CK_user_device_lists_generation_positive", "\"generation\" >= 1");
                    table.CheckConstraint("CK_user_device_lists_issued_at_positive", "\"issued_at_ms\" > 0");
                    table.ForeignKey(
                        name: "FK_user_device_lists_users_user_id",
                        column: x => x.user_id,
                        principalTable: "users",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateIndex(
                name: "IX_user_devices_user_id_revoked_at_deleted_at",
                table: "user_devices",
                columns: new[] { "user_id", "revoked_at", "deleted_at" });

            migrationBuilder.AddCheckConstraint(
                name: "CK_session_key_blobs_issued_at_ms_positive",
                table: "session_key_blobs",
                sql: "\"issued_at_ms\" IS NULL OR \"issued_at_ms\" > 0");

            migrationBuilder.CreateIndex(
                name: "IX_device_link_requests_device_code",
                table: "device_link_requests",
                column: "device_code",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_device_link_requests_expires_at",
                table: "device_link_requests",
                column: "expires_at");

            migrationBuilder.CreateIndex(
                name: "IX_device_link_requests_user_code",
                table: "device_link_requests",
                column: "user_code",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_device_link_requests_user_id",
                table: "device_link_requests",
                column: "user_id");

            migrationBuilder.CreateIndex(
                name: "IX_device_registration_challenges_expires_at",
                table: "device_registration_challenges",
                column: "expires_at");

            migrationBuilder.CreateIndex(
                name: "IX_device_registration_challenges_user_id",
                table: "device_registration_challenges",
                column: "user_id");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "device_link_requests");

            migrationBuilder.DropTable(
                name: "device_registration_challenges");

            migrationBuilder.DropTable(
                name: "user_device_lists");

            migrationBuilder.DropIndex(
                name: "IX_user_devices_user_id_revoked_at_deleted_at",
                table: "user_devices");

            migrationBuilder.DropCheckConstraint(
                name: "CK_session_key_blobs_issued_at_ms_positive",
                table: "session_key_blobs");

            migrationBuilder.DropColumn(
                name: "cert_expires_at",
                table: "user_devices");

            migrationBuilder.DropColumn(
                name: "cert_issued_at",
                table: "user_devices");

            migrationBuilder.DropColumn(
                name: "cert_signer_device_id",
                table: "user_devices");

            migrationBuilder.DropColumn(
                name: "deleted_at",
                table: "user_devices");

            migrationBuilder.DropColumn(
                name: "device_certificate",
                table: "user_devices");

            migrationBuilder.DropColumn(
                name: "device_certificate_signature",
                table: "user_devices");

            migrationBuilder.DropColumn(
                name: "device_label",
                table: "user_devices");

            migrationBuilder.DropColumn(
                name: "revoked_at",
                table: "user_devices");

            migrationBuilder.DropColumn(
                name: "revoked_by_device_id",
                table: "user_devices");

            migrationBuilder.DropColumn(
                name: "aad_context",
                table: "session_key_blobs");

            migrationBuilder.DropColumn(
                name: "issued_at_ms",
                table: "session_key_blobs");

            migrationBuilder.DropColumn(
                name: "host_claimed_at",
                table: "coding_sessions");

            migrationBuilder.DropColumn(
                name: "host_connection_slot",
                table: "coding_sessions");

            migrationBuilder.DropColumn(
                name: "live_participant_count",
                table: "coding_sessions");
        }
    }
}
