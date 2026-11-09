using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class KeepFriendListHistory : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropPrimaryKey(
                name: "PK_friend_lists",
                table: "friend_lists");

            migrationBuilder.AddPrimaryKey(
                name: "PK_friend_lists",
                table: "friend_lists",
                columns: new[] { "UserId", "Revision" });
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropPrimaryKey(
                name: "PK_friend_lists",
                table: "friend_lists");

            migrationBuilder.AddPrimaryKey(
                name: "PK_friend_lists",
                table: "friend_lists",
                column: "UserId");
        }
    }
}
