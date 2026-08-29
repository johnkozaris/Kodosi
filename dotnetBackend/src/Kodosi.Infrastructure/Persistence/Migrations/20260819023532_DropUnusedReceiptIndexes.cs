using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class DropUnusedReceiptIndexes : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_session_end_mutations_session_id_incarnation_id",
                table: "session_end_mutations");

            migrationBuilder.DropIndex(
                name: "IX_session_access_mutations_session_id_incarnation_id",
                table: "session_access_mutations");

            migrationBuilder.DropIndex(
                name: "IX_room_mutation_receipts_room_id_created_at",
                table: "room_mutation_receipts");

            migrationBuilder.CreateIndex(
                name: "IX_room_mutation_receipts_room_id",
                table: "room_mutation_receipts",
                column: "room_id");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "IX_room_mutation_receipts_room_id",
                table: "room_mutation_receipts");

            migrationBuilder.CreateIndex(
                name: "IX_session_end_mutations_session_id_incarnation_id",
                table: "session_end_mutations",
                columns: new[] { "session_id", "incarnation_id" });

            migrationBuilder.CreateIndex(
                name: "IX_session_access_mutations_session_id_incarnation_id",
                table: "session_access_mutations",
                columns: new[] { "session_id", "incarnation_id" });

            migrationBuilder.CreateIndex(
                name: "IX_room_mutation_receipts_room_id_created_at",
                table: "room_mutation_receipts",
                columns: new[] { "room_id", "created_at" });
        }
    }
}
