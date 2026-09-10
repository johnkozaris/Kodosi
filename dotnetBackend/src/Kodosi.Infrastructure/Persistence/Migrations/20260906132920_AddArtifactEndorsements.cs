using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    /// <inheritdoc />
    public partial class AddArtifactEndorsements : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "artifact_endorsements",
                columns: table => new
                {
                    user_id = table.Column<Guid>(type: "uuid", nullable: false),
                    identity_incarnation_id = table.Column<Guid>(type: "uuid", nullable: false),
                    artifact_digest = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    endorser_device_id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    signature = table.Column<byte[]>(type: "bytea", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_artifact_endorsements", x => new { x.user_id, x.identity_incarnation_id, x.artifact_digest });
                    table.CheckConstraint("CK_artifact_endorsements_digest", "artifact_digest ~ '^[0-9a-f]{64}$'");
                    table.CheckConstraint("CK_artifact_endorsements_signature", "octet_length(signature) = 3309");
                    table.ForeignKey(
                        name: "FK_artifact_endorsements_users_user_id",
                        column: x => x.user_id,
                        principalTable: "users",
                        principalColumn: "id",
                        onDelete: ReferentialAction.Cascade);
                });
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "artifact_endorsements");
        }
    }
}
