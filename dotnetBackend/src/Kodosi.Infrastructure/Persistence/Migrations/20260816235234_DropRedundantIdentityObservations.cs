using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class DropRedundantIdentityObservations : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_users_auth_subject",
                table: "users");

            migrationBuilder.DropColumn(
                name: "auth_subject",
                table: "users");

            migrationBuilder.DropColumn(
                name: "avatar_url_snapshot",
                table: "external_identities");

            migrationBuilder.DropColumn(
                name: "display_name_snapshot",
                table: "external_identities");

            migrationBuilder.DropColumn(
                name: "email_snapshot",
                table: "external_identities");

            migrationBuilder.DropColumn(
                name: "email_verified",
                table: "external_identities");

            migrationBuilder.DropColumn(
                name: "last_seen_at",
                table: "external_identities");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $kodosi$
                BEGIN
                    IF EXISTS (SELECT 1 FROM users LIMIT 1)
                        OR EXISTS (SELECT 1 FROM external_identities LIMIT 1) THEN
                        RAISE EXCEPTION
                            'DropRedundantIdentityObservations cannot reconstruct removed identity observations'
                            USING HINT = 'Restore from a backup taken before this migration instead of fabricating identity data.';
                    END IF;
                END
                $kodosi$;
                """);

            migrationBuilder.AddColumn<string>(
                name: "auth_subject",
                table: "users",
                type: "text",
                nullable: false,
                defaultValue: "");

            migrationBuilder.AddColumn<string>(
                name: "avatar_url_snapshot",
                table: "external_identities",
                type: "character varying(2048)",
                maxLength: 2048,
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "display_name_snapshot",
                table: "external_identities",
                type: "character varying(128)",
                maxLength: 128,
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "email_snapshot",
                table: "external_identities",
                type: "character varying(320)",
                maxLength: 320,
                nullable: true);

            migrationBuilder.AddColumn<bool>(
                name: "email_verified",
                table: "external_identities",
                type: "boolean",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "last_seen_at",
                table: "external_identities",
                type: "timestamp with time zone",
                nullable: false,
                defaultValue: new DateTimeOffset(new DateTime(1, 1, 1, 0, 0, 0, 0, DateTimeKind.Unspecified), new TimeSpan(0, 0, 0, 0, 0)));

            migrationBuilder.CreateIndex(
                name: "IX_users_auth_subject",
                table: "users",
                column: "auth_subject",
                unique: true);
        }
    }
}
