using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class AccountDeletion : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.CreateTable(
                name: "deleted_accounts",
                columns: table => new
                {
                    Issuer = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: false),
                    Subject = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: false),
                    DeletedAt = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_deleted_accounts", x => new { x.Issuer, x.Subject });
                });

            migrationBuilder.CreateIndex(
                name: "IX_deleted_accounts_DeletedAt",
                table: "deleted_accounts",
                column: "DeletedAt");
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "deleted_accounts");
        }
    }
}
