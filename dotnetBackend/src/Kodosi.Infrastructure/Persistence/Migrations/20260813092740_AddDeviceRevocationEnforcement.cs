using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddDeviceRevocationEnforcement : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<Guid[]>(
                name: "affected_session_ids",
                table: "device_revocation_audit",
                type: "uuid[]",
                nullable: false,
                defaultValue: new Guid[0]);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "realtime_enforced_at",
                table: "device_revocation_audit",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<Guid>(
                name: "revocation_id",
                table: "device_revocation_audit",
                type: "uuid",
                nullable: true);

            migrationBuilder.Sql(
                """
                UPDATE device_revocation_audit
                SET revocation_id = id,
                    realtime_enforced_at = occurred_at
                WHERE revocation_id IS NULL;
                """);

            migrationBuilder.AlterColumn<Guid>(
                name: "revocation_id",
                table: "device_revocation_audit",
                type: "uuid",
                nullable: false,
                oldClrType: typeof(Guid),
                oldType: "uuid",
                oldNullable: true);

            migrationBuilder.CreateIndex(
                name: "IX_device_revocation_audit_revocation_id",
                table: "device_revocation_audit",
                column: "revocation_id");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM device_revocation_audit
                        WHERE realtime_enforced_at IS NULL
                    ) THEN
                        RAISE EXCEPTION 'Cannot remove device revocation enforcement while pending obligations exist';
                    END IF;
                END $$;
                """);

            migrationBuilder.DropIndex(
                name: "IX_device_revocation_audit_revocation_id",
                table: "device_revocation_audit");

            migrationBuilder.DropColumn(
                name: "affected_session_ids",
                table: "device_revocation_audit");

            migrationBuilder.DropColumn(
                name: "realtime_enforced_at",
                table: "device_revocation_audit");

            migrationBuilder.DropColumn(
                name: "revocation_id",
                table: "device_revocation_audit");
        }
    }
}
