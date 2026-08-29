using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class AddIdentityResetAudience : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<Guid[]>(
                name: "audience_user_ids",
                table: "identity_reset_audit",
                type: "uuid[]",
                nullable: false,
                defaultValue: new Guid[0]);
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropColumn(
                name: "audience_user_ids",
                table: "identity_reset_audit");
        }
    }
}
