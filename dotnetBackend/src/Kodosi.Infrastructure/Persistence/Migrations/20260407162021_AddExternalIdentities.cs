using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddExternalIdentities : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "external_identities",
                columns: table => new
                {
                    id = table.Column<Guid>(type: "uuid", nullable: false),
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    provider = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    issuer = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: false),
                    subject = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: false),
                    email_snapshot = table.Column<string>(type: "character varying(320)", maxLength: 320, nullable: true),
                    email_verified = table.Column<bool>(type: "boolean", nullable: true),
                    display_name_snapshot = table.Column<string>(type: "character varying(128)", maxLength: 128, nullable: true),
                    avatar_url_snapshot = table.Column<string>(type: "character varying(2048)", maxLength: 2048, nullable: true),
                    linked_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    last_seen_at = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_external_identities", x => x.id);
                    table.ForeignKey(
                        name: "FK_external_identities_users_user_id",
                        column: x => x.user_id,
                        principalTable: "users",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateIndex(
                name: "IX_external_identities_provider_issuer_subject",
                table: "external_identities",
                columns: new[] { "provider", "issuer", "subject" },
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_external_identities_user_id",
                table: "external_identities",
                column: "user_id");

            migrationBuilder.Sql(
                """
                INSERT INTO external_identities (
                    id,
                    user_id,
                    provider,
                    issuer,
                    subject,
                    email_snapshot,
                    email_verified,
                    display_name_snapshot,
                    avatar_url_snapshot,
                    linked_at,
                    last_seen_at)
                SELECT
                    id,
                    id,
                    'authentik',
                    'https://auth.kodosi.com/application/o/kodosi/',
                    auth_subject,
                    email,
                    NULL,
                    display_name,
                    avatar_url,
                    created_at,
                    created_at
                FROM users
                ON CONFLICT (provider, issuer, subject) DO NOTHING;

                UPDATE users
                SET auth_subject = CONCAT(
                    'authentik',
                    CHR(10),
                    'https://auth.kodosi.com/application/o/kodosi/',
                    CHR(10),
                    auth_subject);
                """);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "external_identities");
        }
    }
}
