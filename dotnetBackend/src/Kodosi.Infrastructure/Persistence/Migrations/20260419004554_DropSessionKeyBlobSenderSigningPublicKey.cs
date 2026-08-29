using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class DropSessionKeyBlobSenderSigningPublicKey : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropColumn(
                name: "sender_signing_public_key",
                table: "session_key_blobs");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {



            throw new InvalidOperationException(
                "Cannot roll back DropSessionKeyBlobSenderSigningPublicKey automatically: " +
                "the dropped column's per-row values are not recoverable. " +
                "Restore from a pre-drop backup or re-issue session key blobs.");
        }
    }
}
