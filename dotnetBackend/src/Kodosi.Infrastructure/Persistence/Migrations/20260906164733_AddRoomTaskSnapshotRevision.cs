using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    /// <inheritdoc />
    public partial class AddRoomTaskSnapshotRevision : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<long>(
                name: "task_revision",
                table: "rooms",
                type: "bigint",
                nullable: false,
                defaultValue: 0L);

            migrationBuilder.AddCheckConstraint(
                name: "CK_rooms_task_revision_nonnegative",
                table: "rooms",
                sql: "task_revision >= 0");
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "CK_rooms_task_revision_nonnegative",
                table: "rooms");

            migrationBuilder.DropColumn(
                name: "task_revision",
                table: "rooms");
        }
    }
}
