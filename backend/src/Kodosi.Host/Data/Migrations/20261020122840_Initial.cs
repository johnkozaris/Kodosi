using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Kodosi.Data.Migrations
{
    /// <inheritdoc />
    public partial class Initial : Migration
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

            migrationBuilder.CreateTable(
                name: "users",
                columns: table => new
                {
                    Id = table.Column<Guid>(type: "uuid", nullable: false),
                    Issuer = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: false),
                    Subject = table.Column<string>(type: "character varying(512)", maxLength: 512, nullable: false),
                    Handle = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    DisplayName = table.Column<string>(type: "character varying(128)", maxLength: 128, nullable: false),
                    IdentityIncarnationId = table.Column<Guid>(type: "uuid", nullable: true),
                    Plan = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_users", x => x.Id);
                });

            migrationBuilder.CreateTable(
                name: "device_links",
                columns: table => new
                {
                    Id = table.Column<Guid>(type: "uuid", nullable: false),
                    UserId = table.Column<Guid>(type: "uuid", nullable: false),
                    DeviceCodeHash = table.Column<string>(type: "character varying(64)", maxLength: 64, nullable: false),
                    Nonce = table.Column<byte[]>(type: "bytea", nullable: false),
                    Proof = table.Column<byte[]>(type: "bytea", nullable: false),
                    ApprovalProof = table.Column<byte[]>(type: "bytea", nullable: true),
                    DeviceId = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    Label = table.Column<string>(type: "character varying(128)", maxLength: 128, nullable: false),
                    SigningPublicKey = table.Column<byte[]>(type: "bytea", nullable: false),
                    ExpiresAt = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    State = table.Column<string>(type: "character varying(16)", maxLength: 16, nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_device_links", x => x.Id);
                    table.ForeignKey(
                        name: "FK_device_links_users_UserId",
                        column: x => x.UserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateTable(
                name: "device_lists",
                columns: table => new
                {
                    UserId = table.Column<Guid>(type: "uuid", nullable: false),
                    Generation = table.Column<long>(type: "bigint", nullable: false),
                    Body = table.Column<byte[]>(type: "bytea", nullable: false),
                    Signature = table.Column<byte[]>(type: "bytea", nullable: false),
                    IssuedAtMs = table.Column<long>(type: "bigint", nullable: false),
                    ExpiresAtMs = table.Column<long>(type: "bigint", nullable: true)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_device_lists", x => x.UserId);
                    table.ForeignKey(
                        name: "FK_device_lists_users_UserId",
                        column: x => x.UserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "devices",
                columns: table => new
                {
                    Id = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    UserId = table.Column<Guid>(type: "uuid", nullable: false),
                    SignerDeviceId = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    Certificate = table.Column<byte[]>(type: "bytea", nullable: false),
                    CertificateSignature = table.Column<byte[]>(type: "bytea", nullable: false),
                    SigningPublicKey = table.Column<byte[]>(type: "bytea", nullable: false),
                    RecoveryBox = table.Column<byte[]>(type: "bytea", nullable: true),
                    IssuedAtMs = table.Column<long>(type: "bigint", nullable: false),
                    ExpiresAtMs = table.Column<long>(type: "bigint", nullable: true),
                    Revoked = table.Column<bool>(type: "boolean", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_devices", x => x.Id);
                    table.ForeignKey(
                        name: "FK_devices_users_UserId",
                        column: x => x.UserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "friend_lists",
                columns: table => new
                {
                    UserId = table.Column<Guid>(type: "uuid", nullable: false),
                    Revision = table.Column<long>(type: "bigint", nullable: false),
                    Body = table.Column<byte[]>(type: "bytea", nullable: false),
                    Signature = table.Column<byte[]>(type: "bytea", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_friend_lists", x => x.UserId);
                    table.ForeignKey(
                        name: "FK_friend_lists_users_UserId",
                        column: x => x.UserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateTable(
                name: "friendships",
                columns: table => new
                {
                    FirstUserId = table.Column<Guid>(type: "uuid", nullable: false),
                    SecondUserId = table.Column<Guid>(type: "uuid", nullable: false),
                    RequestedBy = table.Column<Guid>(type: "uuid", nullable: false),
                    Accepted = table.Column<bool>(type: "boolean", nullable: false),
                    CreatedAt = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_friendships", x => new { x.FirstUserId, x.SecondUserId });
                    table.ForeignKey(
                        name: "FK_friendships_users_FirstUserId",
                        column: x => x.FirstUserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "FK_friendships_users_SecondUserId",
                        column: x => x.SecondUserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "missions",
                columns: table => new
                {
                    Id = table.Column<Guid>(type: "uuid", nullable: false),
                    Name = table.Column<string>(type: "character varying(128)", maxLength: 128, nullable: false),
                    OwnerUserId = table.Column<Guid>(type: "uuid", nullable: false),
                    Dosi = table.Column<bool>(type: "boolean", nullable: false),
                    KeyVersion = table.Column<long>(type: "bigint", nullable: false),
                    ContentSequence = table.Column<long>(type: "bigint", nullable: false),
                    ContentBytes = table.Column<long>(type: "bigint", nullable: false),
                    HistoryTrimmed = table.Column<bool>(type: "boolean", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_missions", x => x.Id);
                    table.ForeignKey(
                        name: "FK_missions_users_OwnerUserId",
                        column: x => x.OwnerUserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "room_recipient_keys",
                columns: table => new
                {
                    DeviceId = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    UserId = table.Column<Guid>(type: "uuid", nullable: false),
                    PublicKey = table.Column<byte[]>(type: "bytea", nullable: false),
                    Signature = table.Column<byte[]>(type: "bytea", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_room_recipient_keys", x => x.DeviceId);
                    table.ForeignKey(
                        name: "FK_room_recipient_keys_devices_DeviceId",
                        column: x => x.DeviceId,
                        principalTable: "devices",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                    table.ForeignKey(
                        name: "FK_room_recipient_keys_users_UserId",
                        column: x => x.UserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateTable(
                name: "mission_invitations",
                columns: table => new
                {
                    Id = table.Column<Guid>(type: "uuid", nullable: false),
                    MissionId = table.Column<Guid>(type: "uuid", nullable: false),
                    UserId = table.Column<Guid>(type: "uuid", nullable: false),
                    InviterUserId = table.Column<Guid>(type: "uuid", nullable: false),
                    CreatedAt = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_mission_invitations", x => x.Id);
                    table.ForeignKey(
                        name: "FK_mission_invitations_missions_MissionId",
                        column: x => x.MissionId,
                        principalTable: "missions",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                    table.ForeignKey(
                        name: "FK_mission_invitations_users_UserId",
                        column: x => x.UserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "mission_items",
                columns: table => new
                {
                    MissionId = table.Column<Guid>(type: "uuid", nullable: false),
                    Id = table.Column<Guid>(type: "uuid", nullable: false),
                    Kind = table.Column<string>(type: "character varying(16)", maxLength: 16, nullable: false),
                    Version = table.Column<long>(type: "bigint", nullable: false),
                    Sequence = table.Column<long>(type: "bigint", nullable: false),
                    KeyVersion = table.Column<long>(type: "bigint", nullable: false),
                    Body = table.Column<byte[]>(type: "bytea", nullable: false),
                    Signature = table.Column<byte[]>(type: "bytea", nullable: false),
                    UserId = table.Column<Guid>(type: "uuid", nullable: false),
                    DeviceId = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    CreatedAt = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_mission_items", x => new { x.MissionId, x.Id });
                    table.ForeignKey(
                        name: "FK_mission_items_missions_MissionId",
                        column: x => x.MissionId,
                        principalTable: "missions",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateTable(
                name: "mission_key_states",
                columns: table => new
                {
                    MissionId = table.Column<Guid>(type: "uuid", nullable: false),
                    Version = table.Column<long>(type: "bigint", nullable: false),
                    Epoch = table.Column<long>(type: "bigint", nullable: false),
                    Body = table.Column<byte[]>(type: "bytea", nullable: false),
                    Signature = table.Column<byte[]>(type: "bytea", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_mission_key_states", x => new { x.MissionId, x.Version });
                    table.ForeignKey(
                        name: "FK_mission_key_states_missions_MissionId",
                        column: x => x.MissionId,
                        principalTable: "missions",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                });

            migrationBuilder.CreateTable(
                name: "mission_members",
                columns: table => new
                {
                    MissionId = table.Column<Guid>(type: "uuid", nullable: false),
                    UserId = table.Column<Guid>(type: "uuid", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_mission_members", x => new { x.MissionId, x.UserId });
                    table.ForeignKey(
                        name: "FK_mission_members_missions_MissionId",
                        column: x => x.MissionId,
                        principalTable: "missions",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                    table.ForeignKey(
                        name: "FK_mission_members_users_UserId",
                        column: x => x.UserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "sessions",
                columns: table => new
                {
                    Id = table.Column<Guid>(type: "uuid", nullable: false),
                    IncarnationId = table.Column<Guid>(type: "uuid", nullable: false),
                    OwnerUserId = table.Column<Guid>(type: "uuid", nullable: false),
                    HostDeviceId = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: false),
                    HostName = table.Column<string>(type: "character varying(128)", maxLength: 128, nullable: false),
                    Name = table.Column<string>(type: "character varying(128)", maxLength: 128, nullable: false),
                    MissionId = table.Column<Guid>(type: "uuid", nullable: true),
                    RoomSequence = table.Column<long>(type: "bigint", nullable: false),
                    RoomDeviceId = table.Column<string>(type: "character varying(256)", maxLength: 256, nullable: true),
                    RoomSignature = table.Column<byte[]>(type: "bytea", nullable: true),
                    AuthorizationRevision = table.Column<long>(type: "bigint", nullable: false),
                    Ended = table.Column<bool>(type: "boolean", nullable: false),
                    ExpiresAt = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false),
                    CreatedAt = table.Column<DateTimeOffset>(type: "timestamp with time zone", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_sessions", x => x.Id);
                    table.ForeignKey(
                        name: "FK_sessions_devices_HostDeviceId",
                        column: x => x.HostDeviceId,
                        principalTable: "devices",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                    table.ForeignKey(
                        name: "FK_sessions_missions_MissionId",
                        column: x => x.MissionId,
                        principalTable: "missions",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.SetNull);
                    table.ForeignKey(
                        name: "FK_sessions_users_OwnerUserId",
                        column: x => x.OwnerUserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateTable(
                name: "session_members",
                columns: table => new
                {
                    SessionId = table.Column<Guid>(type: "uuid", nullable: false),
                    UserId = table.Column<Guid>(type: "uuid", nullable: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_session_members", x => new { x.SessionId, x.UserId });
                    table.ForeignKey(
                        name: "FK_session_members_sessions_SessionId",
                        column: x => x.SessionId,
                        principalTable: "sessions",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Cascade);
                    table.ForeignKey(
                        name: "FK_session_members_users_UserId",
                        column: x => x.UserId,
                        principalTable: "users",
                        principalColumn: "Id",
                        onDelete: ReferentialAction.Restrict);
                });

            migrationBuilder.CreateIndex(
                name: "IX_deleted_accounts_DeletedAt",
                table: "deleted_accounts",
                column: "DeletedAt");

            migrationBuilder.CreateIndex(
                name: "IX_device_links_DeviceCodeHash",
                table: "device_links",
                column: "DeviceCodeHash",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_device_links_ExpiresAt",
                table: "device_links",
                column: "ExpiresAt");

            migrationBuilder.CreateIndex(
                name: "IX_device_links_UserId",
                table: "device_links",
                column: "UserId");

            migrationBuilder.CreateIndex(
                name: "IX_devices_UserId",
                table: "devices",
                column: "UserId");

            migrationBuilder.CreateIndex(
                name: "IX_friendships_SecondUserId",
                table: "friendships",
                column: "SecondUserId");

            migrationBuilder.CreateIndex(
                name: "IX_mission_invitations_MissionId_UserId",
                table: "mission_invitations",
                columns: new[] { "MissionId", "UserId" },
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_mission_invitations_UserId",
                table: "mission_invitations",
                column: "UserId");

            migrationBuilder.CreateIndex(
                name: "IX_mission_items_MissionId_Kind_Sequence",
                table: "mission_items",
                columns: new[] { "MissionId", "Kind", "Sequence" });

            migrationBuilder.CreateIndex(
                name: "IX_mission_items_MissionId_Sequence",
                table: "mission_items",
                columns: new[] { "MissionId", "Sequence" },
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_mission_members_UserId",
                table: "mission_members",
                column: "UserId");

            migrationBuilder.CreateIndex(
                name: "IX_missions_Dosi_OwnerUserId",
                table: "missions",
                columns: new[] { "Dosi", "OwnerUserId" },
                unique: true,
                filter: "\"Dosi\"");

            migrationBuilder.CreateIndex(
                name: "IX_missions_OwnerUserId",
                table: "missions",
                column: "OwnerUserId");

            migrationBuilder.CreateIndex(
                name: "IX_room_recipient_keys_UserId",
                table: "room_recipient_keys",
                column: "UserId");

            migrationBuilder.CreateIndex(
                name: "IX_session_members_UserId",
                table: "session_members",
                column: "UserId");

            migrationBuilder.CreateIndex(
                name: "IX_sessions_Ended_ExpiresAt",
                table: "sessions",
                columns: new[] { "Ended", "ExpiresAt" });

            migrationBuilder.CreateIndex(
                name: "IX_sessions_HostDeviceId",
                table: "sessions",
                column: "HostDeviceId");

            migrationBuilder.CreateIndex(
                name: "IX_sessions_IncarnationId",
                table: "sessions",
                column: "IncarnationId",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_sessions_MissionId",
                table: "sessions",
                column: "MissionId");

            migrationBuilder.CreateIndex(
                name: "IX_sessions_OwnerUserId_Ended",
                table: "sessions",
                columns: new[] { "OwnerUserId", "Ended" });

            migrationBuilder.CreateIndex(
                name: "IX_users_Handle",
                table: "users",
                column: "Handle",
                unique: true);

            migrationBuilder.CreateIndex(
                name: "IX_users_Issuer_Subject",
                table: "users",
                columns: new[] { "Issuer", "Subject" },
                unique: true);
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "deleted_accounts");

            migrationBuilder.DropTable(
                name: "device_links");

            migrationBuilder.DropTable(
                name: "device_lists");

            migrationBuilder.DropTable(
                name: "friend_lists");

            migrationBuilder.DropTable(
                name: "friendships");

            migrationBuilder.DropTable(
                name: "mission_invitations");

            migrationBuilder.DropTable(
                name: "mission_items");

            migrationBuilder.DropTable(
                name: "mission_key_states");

            migrationBuilder.DropTable(
                name: "mission_members");

            migrationBuilder.DropTable(
                name: "room_recipient_keys");

            migrationBuilder.DropTable(
                name: "session_members");

            migrationBuilder.DropTable(
                name: "sessions");

            migrationBuilder.DropTable(
                name: "devices");

            migrationBuilder.DropTable(
                name: "missions");

            migrationBuilder.DropTable(
                name: "users");
        }
    }
}
