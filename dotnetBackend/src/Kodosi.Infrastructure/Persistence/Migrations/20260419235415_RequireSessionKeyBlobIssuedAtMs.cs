using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class RequireSessionKeyBlobIssuedAtMs : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {



            migrationBuilder.Sql(
                "DELETE FROM session_key_blobs WHERE \"issued_at_ms\" IS NULL;");

            migrationBuilder.DropCheckConstraint(
                name: "CK_session_key_blobs_issued_at_ms_positive",
                table: "session_key_blobs");

            migrationBuilder.AlterColumn<long>(
                name: "issued_at_ms",
                table: "session_key_blobs",
                type: "bigint",
                nullable: false,
                oldClrType: typeof(long),
                oldType: "bigint",
                oldNullable: true);



            migrationBuilder.AddColumn<uint>(
                name: "xmin",
                table: "friendships",
                type: "xid",
                rowVersion: true,
                nullable: false,
                defaultValue: 0u);

            migrationBuilder.AddCheckConstraint(
                name: "CK_session_key_blobs_issued_at_ms_positive",
                table: "session_key_blobs",
                sql: "\"issued_at_ms\" > 0");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_session_key_blobs_issued_at_ms_positive",
                table: "session_key_blobs");

            migrationBuilder.DropColumn(
                name: "xmin",
                table: "friendships");

            migrationBuilder.AlterColumn<long>(
                name: "issued_at_ms",
                table: "session_key_blobs",
                type: "bigint",
                nullable: true,
                oldClrType: typeof(long),
                oldType: "bigint");

            migrationBuilder.AddCheckConstraint(
                name: "CK_session_key_blobs_issued_at_ms_positive",
                table: "session_key_blobs",
                sql: "\"issued_at_ms\" IS NULL OR \"issued_at_ms\" > 0");
        }
    }
}
