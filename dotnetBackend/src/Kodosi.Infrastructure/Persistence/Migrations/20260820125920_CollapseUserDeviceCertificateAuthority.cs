using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    public partial class CollapseUserDeviceCertificateAuthority : Migration
    {
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $$
                DECLARE
                    device RECORD;
                    body BYTEA;
                    body_length INTEGER;
                    body_offset INTEGER;
                    field_length INTEGER;
                    certificate_user_id TEXT;
                    certificate_device_id TEXT;
                    certificate_label TEXT;
                    certificate_signer_id TEXT;
                    certificate_kem_key BYTEA;
                    certificate_signing_key BYTEA;
                    certificate_issued_at_ms NUMERIC;
                    certificate_expires_at_ms NUMERIC;
                BEGIN
                    FOR device IN SELECT * FROM user_devices LOOP
                        body := device.device_certificate;
                        body_length := octet_length(body);
                        IF body IS NULL
                           OR body_length NOT BETWEEN 1 AND 5772
                           OR device.device_certificate_signature IS NULL
                           OR octet_length(device.device_certificate_signature) <> 3309 THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: an enrolled device lacks a canonical bounded proof payload';
                        END IF;

                        body_offset := 0;

                        IF body_offset + 4 > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: truncated certificate user ID';
                        END IF;
                        field_length := get_byte(body, body_offset) * 16777216
                            + get_byte(body, body_offset + 1) * 65536
                            + get_byte(body, body_offset + 2) * 256
                            + get_byte(body, body_offset + 3);
                        body_offset := body_offset + 4;
                        IF field_length > 65536 OR body_offset + field_length > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: invalid certificate user ID length';
                        END IF;
                        certificate_user_id := convert_from(
                            substring(body FROM body_offset + 1 FOR field_length), 'UTF8');
                        body_offset := body_offset + field_length;

                        IF body_offset + 4 > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: truncated certificate device ID';
                        END IF;
                        field_length := get_byte(body, body_offset) * 16777216
                            + get_byte(body, body_offset + 1) * 65536
                            + get_byte(body, body_offset + 2) * 256
                            + get_byte(body, body_offset + 3);
                        body_offset := body_offset + 4;
                        IF field_length > 65536 OR body_offset + field_length > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: invalid certificate device ID length';
                        END IF;
                        certificate_device_id := convert_from(
                            substring(body FROM body_offset + 1 FOR field_length), 'UTF8');
                        body_offset := body_offset + field_length;

                        IF body_offset + 4 > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: truncated certificate label';
                        END IF;
                        field_length := get_byte(body, body_offset) * 16777216
                            + get_byte(body, body_offset + 1) * 65536
                            + get_byte(body, body_offset + 2) * 256
                            + get_byte(body, body_offset + 3);
                        body_offset := body_offset + 4;
                        IF field_length > 65536 OR body_offset + field_length > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: invalid certificate label length';
                        END IF;
                        certificate_label := convert_from(
                            substring(body FROM body_offset + 1 FOR field_length), 'UTF8');
                        body_offset := body_offset + field_length;

                        IF body_offset + 4 > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: truncated certificate signer ID';
                        END IF;
                        field_length := get_byte(body, body_offset) * 16777216
                            + get_byte(body, body_offset + 1) * 65536
                            + get_byte(body, body_offset + 2) * 256
                            + get_byte(body, body_offset + 3);
                        body_offset := body_offset + 4;
                        IF field_length > 65536 OR body_offset + field_length > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: invalid certificate signer ID length';
                        END IF;
                        certificate_signer_id := convert_from(
                            substring(body FROM body_offset + 1 FOR field_length), 'UTF8');
                        body_offset := body_offset + field_length;

                        IF body_offset + 4 > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: truncated certificate KEM key';
                        END IF;
                        field_length := get_byte(body, body_offset) * 16777216
                            + get_byte(body, body_offset + 1) * 65536
                            + get_byte(body, body_offset + 2) * 256
                            + get_byte(body, body_offset + 3);
                        body_offset := body_offset + 4;
                        IF field_length <> 1184 OR body_offset + field_length > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: invalid certificate KEM key length';
                        END IF;
                        certificate_kem_key := substring(
                            body FROM body_offset + 1 FOR field_length);
                        body_offset := body_offset + field_length;

                        IF body_offset + 4 > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: truncated certificate signing key';
                        END IF;
                        field_length := get_byte(body, body_offset) * 16777216
                            + get_byte(body, body_offset + 1) * 65536
                            + get_byte(body, body_offset + 2) * 256
                            + get_byte(body, body_offset + 3);
                        body_offset := body_offset + 4;
                        IF field_length <> 1952 OR body_offset + field_length > body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: invalid certificate signing key length';
                        END IF;
                        certificate_signing_key := substring(
                            body FROM body_offset + 1 FOR field_length);
                        body_offset := body_offset + field_length;

                        IF body_offset + 16 <> body_length THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: certificate timestamps are truncated or trailing bytes are present';
                        END IF;
                        certificate_issued_at_ms :=
                            get_byte(body, body_offset)::NUMERIC * 72057594037927936
                            + get_byte(body, body_offset + 1)::NUMERIC * 281474976710656
                            + get_byte(body, body_offset + 2)::NUMERIC * 1099511627776
                            + get_byte(body, body_offset + 3)::NUMERIC * 4294967296
                            + get_byte(body, body_offset + 4)::NUMERIC * 16777216
                            + get_byte(body, body_offset + 5)::NUMERIC * 65536
                            + get_byte(body, body_offset + 6)::NUMERIC * 256
                            + get_byte(body, body_offset + 7)::NUMERIC;
                        body_offset := body_offset + 8;
                        certificate_expires_at_ms :=
                            get_byte(body, body_offset)::NUMERIC * 72057594037927936
                            + get_byte(body, body_offset + 1)::NUMERIC * 281474976710656
                            + get_byte(body, body_offset + 2)::NUMERIC * 1099511627776
                            + get_byte(body, body_offset + 3)::NUMERIC * 4294967296
                            + get_byte(body, body_offset + 4)::NUMERIC * 16777216
                            + get_byte(body, body_offset + 5)::NUMERIC * 65536
                            + get_byte(body, body_offset + 6)::NUMERIC * 256
                            + get_byte(body, body_offset + 7)::NUMERIC;

                        IF certificate_user_id IS DISTINCT FROM device.user_id::TEXT
                           OR certificate_device_id IS DISTINCT FROM device.device_id
                           OR certificate_device_id = ''
                           OR certificate_device_id <> btrim(certificate_device_id)
                           OR char_length(certificate_device_id) > 256
                           OR certificate_label IS DISTINCT FROM device.device_label
                           OR certificate_label = ''
                           OR certificate_label <> btrim(certificate_label)
                           OR char_length(certificate_label) > 128
                           OR certificate_signer_id IS DISTINCT FROM device.cert_signer_device_id
                           OR certificate_signer_id = ''
                           OR certificate_signer_id <> btrim(certificate_signer_id)
                           OR char_length(certificate_signer_id) > 256
                           OR certificate_kem_key IS DISTINCT FROM device.kem_public_key
                           OR certificate_signing_key IS DISTINCT FROM device.signing_public_key
                           OR certificate_issued_at_ms > 253402300799999
                           OR certificate_expires_at_ms > 253402300799999
                           OR (certificate_expires_at_ms <> 0
                              AND certificate_expires_at_ms <= certificate_issued_at_ms)
                           OR extract(epoch FROM device.cert_issued_at) * 1000
                              IS DISTINCT FROM certificate_issued_at_ms
                           OR (CASE
                                WHEN certificate_expires_at_ms = 0 THEN
                                    device.cert_expires_at IS NOT NULL
                                ELSE extract(epoch FROM device.cert_expires_at) * 1000
                                     IS DISTINCT FROM certificate_expires_at_ms
                              END) THEN
                            RAISE EXCEPTION 'Cannot collapse user-device certificate authority: exact certificate semantics differ from the legacy projections';
                        END IF;
                    END LOOP;
                END $$;
                """);

            migrationBuilder.Sql(
                """
                DO $$
                BEGIN
                    IF EXISTS (
                        SELECT 1
                        FROM user_device_lists
                        WHERE octet_length(body) NOT BETWEEN 1 AND 527432
                           OR octet_length(signature) <> 3309
                    ) THEN
                        RAISE EXCEPTION 'Cannot enforce identity-wire v3 device-list proof bounds: a persisted list has a noncanonical body or signature length';
                    END IF;
                END $$;
                """);

            migrationBuilder.DropCheckConstraint(
                name: "CK_user_device_lists_body_bounded",
                table: "user_device_lists");

            migrationBuilder.DropCheckConstraint(
                name: "CK_user_device_lists_signature_nonempty",
                table: "user_device_lists");

            migrationBuilder.DropColumn(name: "cert_expires_at", table: "user_devices");
            migrationBuilder.DropColumn(name: "cert_issued_at", table: "user_devices");
            migrationBuilder.DropColumn(name: "cert_signer_device_id", table: "user_devices");
            migrationBuilder.DropColumn(name: "device_label", table: "user_devices");
            migrationBuilder.DropColumn(name: "kem_public_key", table: "user_devices");
            migrationBuilder.DropColumn(name: "signing_public_key", table: "user_devices");

            migrationBuilder.AlterColumn<byte[]>(
                name: "device_certificate_signature",
                table: "user_devices",
                type: "bytea",
                maxLength: 3309,
                nullable: false,
                oldClrType: typeof(byte[]),
                oldType: "bytea",
                oldNullable: true);

            migrationBuilder.AlterColumn<byte[]>(
                name: "device_certificate",
                table: "user_devices",
                type: "bytea",
                nullable: false,
                oldClrType: typeof(byte[]),
                oldType: "bytea",
                oldNullable: true);

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_devices_certificate_bounded",
                table: "user_devices",
                sql: "octet_length(\"device_certificate\") BETWEEN 1 AND 5772");

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_devices_certificate_signature_exact",
                table: "user_devices",
                sql: "octet_length(\"device_certificate_signature\") = 3309");

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_device_lists_body_bounded",
                table: "user_device_lists",
                sql: "octet_length(\"body\") <= 527432");

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_device_lists_signature_exact",
                table: "user_device_lists",
                sql: "octet_length(\"signature\") = 3309");
        }

        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.Sql(
                """
                DO $$
                BEGIN
                    IF EXISTS (SELECT 1 FROM user_devices) THEN
                        RAISE EXCEPTION 'Cannot restore removed user-device certificate projections without fabricating signed semantics';
                    END IF;
                END $$;
                """);

            migrationBuilder.DropCheckConstraint(
                name: "CK_user_devices_certificate_bounded",
                table: "user_devices");

            migrationBuilder.DropCheckConstraint(
                name: "CK_user_devices_certificate_signature_exact",
                table: "user_devices");

            migrationBuilder.DropCheckConstraint(
                name: "CK_user_device_lists_body_bounded",
                table: "user_device_lists");

            migrationBuilder.DropCheckConstraint(
                name: "CK_user_device_lists_signature_exact",
                table: "user_device_lists");

            migrationBuilder.AlterColumn<byte[]>(
                name: "device_certificate_signature",
                table: "user_devices",
                type: "bytea",
                nullable: true,
                oldClrType: typeof(byte[]),
                oldType: "bytea",
                oldMaxLength: 3309);

            migrationBuilder.AlterColumn<byte[]>(
                name: "device_certificate",
                table: "user_devices",
                type: "bytea",
                nullable: true,
                oldClrType: typeof(byte[]),
                oldType: "bytea");

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "cert_expires_at",
                table: "user_devices",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "cert_issued_at",
                table: "user_devices",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "cert_signer_device_id",
                table: "user_devices",
                type: "character varying(256)",
                maxLength: 256,
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "device_label",
                table: "user_devices",
                type: "character varying(128)",
                maxLength: 128,
                nullable: true);

            migrationBuilder.AddColumn<byte[]>(
                name: "kem_public_key",
                table: "user_devices",
                type: "bytea",
                maxLength: 1184,
                nullable: false);

            migrationBuilder.AddColumn<byte[]>(
                name: "signing_public_key",
                table: "user_devices",
                type: "bytea",
                maxLength: 1952,
                nullable: false);

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_device_lists_body_bounded",
                table: "user_device_lists",
                sql: "octet_length(\"body\") <= 33687588");

            migrationBuilder.AddCheckConstraint(
                name: "CK_user_device_lists_signature_nonempty",
                table: "user_device_lists",
                sql: "octet_length(\"signature\") > 0");
        }
    }
}
