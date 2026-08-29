using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class DropDuplicateDeviceLinkCertificateReceipt : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropColumn(
                name: "cert_signer_device_id",
                table: "device_link_requests");

            migrationBuilder.DropColumn(
                name: "device_certificate",
                table: "device_link_requests");

            migrationBuilder.DropColumn(
                name: "device_certificate_signature",
                table: "device_link_requests");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<string>(
                name: "cert_signer_device_id",
                table: "device_link_requests",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "device_certificate",
                table: "device_link_requests",
                type: "bytea",
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "device_certificate_signature",
                table: "device_link_requests",
                type: "bytea",
                nullable: true);
        }
    }
}
