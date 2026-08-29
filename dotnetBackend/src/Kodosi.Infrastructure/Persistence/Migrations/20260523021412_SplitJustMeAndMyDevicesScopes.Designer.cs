
using System;
using Kodosi.Infrastructure.Persistence;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Infrastructure;
using Microsoft.EntityFrameworkCore.Migrations;
using Microsoft.EntityFrameworkCore.Storage.ValueConversion;
using Npgsql.EntityFrameworkCore.PostgreSQL.Metadata;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    [DbContext(typeof(KodosiDbContext))]
    [Migration("20260523021412_SplitJustMeAndMyDevicesScopes")]
    partial class SplitJustMeAndMyDevicesScopes
    {
                protected override void BuildTargetModel(ModelBuilder modelBuilder)
        {
#pragma warning disable 612, 618
            modelBuilder
                .HasAnnotation("ProductVersion", "10.0.7")
                .HasAnnotation("Relational:MaxIdentifierLength", 63);

            NpgsqlModelBuilderExtensions.UseIdentityByDefaultColumns(modelBuilder);

            modelBuilder.Entity("Kodosi.Domain.AccessOverrideAuditEntry", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<string>("Action")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("action");

                    b.Property<Guid>("ActorUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("actor_user_id");

                    b.Property<string>("ClientIp")
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("client_ip");

                    b.Property<Guid>("GranteeUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("grantee_user_id");

                    b.Property<DateTimeOffset>("OccurredAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("occurred_at");

                    b.Property<string>("Reason")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("reason");

                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_id");

                    b.Property<string>("UserAgent")
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("user_agent");

                    b.HasKey("Id");

                    b.HasIndex("ActorUserId");

                    b.HasIndex("GranteeUserId");

                    b.HasIndex("OccurredAt");

                    b.HasIndex("SessionId");

                    b.ToTable("access_override_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.DeviceLinkRequest", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<DateTimeOffset?>("ApprovedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("approved_at");

                    b.Property<DateTimeOffset?>("CancelledAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("cancelled_at");

                    b.Property<string>("CertSignerDeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("cert_signer_device_id");

                    b.Property<DateTimeOffset?>("ConsumedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("consumed_at");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<byte[]>("DeviceCertificate")
                        .HasColumnType("bytea")
                        .HasColumnName("device_certificate");

                    b.Property<byte[]>("DeviceCertificateSignature")
                        .HasColumnType("bytea")
                        .HasColumnName("device_certificate_signature");

                    b.Property<string>("DeviceCode")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("device_code");

                    b.Property<string>("DeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("device_id");

                    b.Property<string>("DeviceLabel")
                        .IsRequired()
                        .HasMaxLength(128)
                        .HasColumnType("character varying(128)")
                        .HasColumnName("device_label");

                    b.Property<long?>("DeviceListGeneration")
                        .HasColumnType("bigint")
                        .HasColumnName("device_list_generation");

                    b.Property<DateTimeOffset>("ExpiresAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("expires_at");

                    b.Property<byte[]>("KemPublicKey")
                        .IsRequired()
                        .HasMaxLength(1184)
                        .HasColumnType("bytea")
                        .HasColumnName("kem_public_key");

                    b.Property<byte[]>("SigningPublicKey")
                        .IsRequired()
                        .HasMaxLength(1952)
                        .HasColumnType("bytea")
                        .HasColumnName("signing_public_key");

                    b.Property<string>("UserCode")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("user_code");

                    b.Property<Guid>("UserId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_id");

                    b.HasKey("Id");

                    b.HasIndex("DeviceCode")
                        .IsUnique();

                    b.HasIndex("ExpiresAt");

                    b.HasIndex("UserCode")
                        .IsUnique();

                    b.HasIndex("UserId");

                    b.ToTable("device_link_requests", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.DeviceRegistrationChallenge", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<byte[]>("Challenge")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("challenge");

                    b.Property<DateTimeOffset?>("ConsumedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("consumed_at");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<DateTimeOffset>("ExpiresAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("expires_at");

                    b.Property<Guid>("UserId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_id");

                    b.Property<uint>("Version")
                        .IsConcurrencyToken()
                        .ValueGeneratedOnAddOrUpdate()
                        .HasColumnType("xid")
                        .HasColumnName("xmin");

                    b.HasKey("Id");

                    b.HasIndex("ExpiresAt");

                    b.HasIndex("UserId");

                    b.ToTable("device_registration_challenges", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.DeviceRevocationAuditEntry", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<Guid>("ActorUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("actor_user_id");

                    b.Property<int>("BlobsCascaded")
                        .HasColumnType("integer")
                        .HasColumnName("blobs_cascaded");

                    b.Property<string>("ClientIp")
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("client_ip");

                    b.Property<int>("NewGeneration")
                        .HasColumnType("integer")
                        .HasColumnName("new_generation");

                    b.Property<DateTimeOffset>("OccurredAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("occurred_at");

                    b.Property<string>("RevokedDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("revoked_device_id");

                    b.Property<string>("SignerDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("signer_device_id");

                    b.Property<string>("UserAgent")
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("user_agent");

                    b.HasKey("Id");

                    b.HasIndex("ActorUserId");

                    b.HasIndex("OccurredAt");

                    b.HasIndex("RevokedDeviceId");

                    b.ToTable("device_revocation_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.ExternalIdentity", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<string>("AvatarUrlSnapshot")
                        .HasMaxLength(2048)
                        .HasColumnType("character varying(2048)")
                        .HasColumnName("avatar_url_snapshot");

                    b.Property<string>("DisplayNameSnapshot")
                        .HasMaxLength(128)
                        .HasColumnType("character varying(128)")
                        .HasColumnName("display_name_snapshot");

                    b.Property<string>("EmailSnapshot")
                        .HasMaxLength(320)
                        .HasColumnType("character varying(320)")
                        .HasColumnName("email_snapshot");

                    b.Property<bool?>("EmailVerified")
                        .HasColumnType("boolean")
                        .HasColumnName("email_verified");

                    b.Property<string>("Issuer")
                        .IsRequired()
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("issuer");

                    b.Property<DateTimeOffset>("LastSeenAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("last_seen_at");

                    b.Property<DateTimeOffset>("LinkedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("linked_at");

                    b.Property<string>("Provider")
                        .IsRequired()
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("provider");

                    b.Property<string>("Subject")
                        .IsRequired()
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("subject");

                    b.Property<Guid>("UserId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_id");

                    b.HasKey("Id");

                    b.HasIndex("UserId");

                    b.HasIndex("Provider", "Issuer", "Subject")
                        .IsUnique();

                    b.ToTable("external_identities", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.Friendship", b =>
                {
                    b.Property<Guid>("UserLowId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_low_id");

                    b.Property<Guid>("UserHighId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_high_id");

                    b.Property<DateTimeOffset?>("AcceptedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("accepted_at");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<Guid>("RequestorUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("requestor_user_id");

                    b.Property<string>("Status")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("status");

                    b.Property<uint>("Version")
                        .IsConcurrencyToken()
                        .ValueGeneratedOnAddOrUpdate()
                        .HasColumnType("xid")
                        .HasColumnName("xmin");

                    b.HasKey("UserLowId", "UserHighId");

                    b.HasIndex("RequestorUserId");

                    b.HasIndex("UserHighId");

                    b.HasIndex("Status", "RequestorUserId");

                    b.HasIndex("Status", "UserHighId");

                    b.HasIndex("Status", "UserLowId");

                    b.ToTable("friendships", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.FriendshipAuditEntry", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<string>("Action")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("action");

                    b.Property<Guid>("ActorUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("actor_user_id");

                    b.Property<string>("ClientIp")
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("client_ip");

                    b.Property<DateTimeOffset>("OccurredAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("occurred_at");

                    b.Property<Guid>("OtherUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("other_user_id");

                    b.Property<string>("UserAgent")
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("user_agent");

                    b.HasKey("Id");

                    b.HasIndex("ActorUserId");

                    b.HasIndex("OccurredAt");

                    b.HasIndex("OtherUserId");

                    b.ToTable("friendship_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.IdentityResetAuditEntry", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<string>("ClientIp")
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("client_ip");

                    b.Property<int>("DeviceListsRemoved")
                        .HasColumnType("integer")
                        .HasColumnName("device_lists_removed");

                    b.Property<int>("DevicesRemoved")
                        .HasColumnType("integer")
                        .HasColumnName("devices_removed");

                    b.Property<DateTimeOffset>("ResetAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("reset_at");

                    b.Property<int>("SessionsAttempted")
                        .HasColumnType("integer")
                        .HasColumnName("sessions_attempted");

                    b.Property<int>("SessionsEnded")
                        .HasColumnType("integer")
                        .HasColumnName("sessions_ended");

                    b.Property<string>("UserAgent")
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("user_agent");

                    b.Property<Guid>("UserId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_id");

                    b.HasKey("Id");

                    b.HasIndex("ResetAt");

                    b.HasIndex("UserId");

                    b.ToTable("identity_reset_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.InputAuditEntry", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<string>("ClientCommandId")
                        .IsRequired()
                        .HasMaxLength(128)
                        .HasColumnType("character varying(128)")
                        .HasColumnName("client_command_id");

                    b.Property<DateTimeOffset?>("CompletedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("completed_at");

                    b.Property<DateTimeOffset?>("DispatchedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("dispatched_at");

                    b.Property<string>("Kind")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("kind");

                    b.Property<int>("PayloadBytesLen")
                        .HasColumnType("integer")
                        .HasColumnName("payload_bytes_len");

                    b.Property<string>("PayloadSha256")
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("payload_sha256");

                    b.Property<Guid>("SenderUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("sender_user_id");

                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_id");

                    b.Property<string>("Status")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("status");

                    b.Property<DateTimeOffset>("SubmittedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("submitted_at");

                    b.HasKey("Id");

                    b.HasIndex("SenderUserId");

                    b.HasIndex("SessionId");

                    b.HasIndex("SessionId", "SenderUserId", "ClientCommandId")
                        .IsUnique();

                    b.ToTable("input_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.Session", b =>
                {
                    b.Property<Guid>("Id")
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<int>("CurrentKeyGeneration")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("integer")
                        .HasDefaultValue(0)
                        .HasColumnName("current_key_generation");

                    b.Property<string>("DefaultAccess")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("default_access");

                    b.Property<DateTimeOffset?>("EndedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("ended_at");

                    b.Property<DateTimeOffset?>("HostClaimedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("host_claimed_at");

                    b.Property<string>("HostConnectionSlot")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("host_connection_slot");

                    b.Property<DateTimeOffset?>("HostReleasedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("host_released_at");

                    b.Property<DateTimeOffset>("LastHeartbeatAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("last_heartbeat_at");

                    b.Property<int>("LiveParticipantCount")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("integer")
                        .HasDefaultValue(0)
                        .HasColumnName("live_participant_count");

                    b.Property<string>("OwnerSessionSecretHash")
                        .IsRequired()
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("owner_session_secret_hash");

                    b.Property<Guid>("OwnerUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("owner_user_id");

                    b.Property<string>("Scope")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("scope");

                    b.Property<DateTimeOffset>("StartedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("started_at");

                    b.Property<string>("Status")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("status");

                    b.Property<string>("Title")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("title");

                    b.Property<string>("ToolKind")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("tool_kind");

                    b.Property<uint>("Version")
                        .IsConcurrencyToken()
                        .ValueGeneratedOnAddOrUpdate()
                        .HasColumnType("xid")
                        .HasColumnName("xmin");

                    b.Property<Guid?>("WorkspaceId")
                        .HasColumnType("uuid")
                        .HasColumnName("workspace_id");

                    b.HasKey("Id");

                    b.HasIndex("OwnerUserId");

                    b.HasIndex("Status");

                    b.HasIndex("WorkspaceId");

                    b.HasIndex("Scope", "Status");

                    b.HasIndex("OwnerUserId", "Status", "Scope");

                    b.ToTable("sessions", null, t =>
                        {
                            t.HasCheckConstraint("CK_sessions_scope_allowed_values", "\"scope\" IN ('Private', 'Friends', 'Workspace')");

                            t.HasCheckConstraint("CK_sessions_workspace_scope_matches_workspace_id", "(\"scope\" = 'Workspace' AND \"workspace_id\" IS NOT NULL)\nOR (\"scope\" <> 'Workspace' AND \"workspace_id\" IS NULL)");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.SessionAccessOverride", b =>
                {
                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_id");

                    b.Property<Guid>("ActorUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("actor_user_id");

                    b.Property<string>("AccessLevel")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("access_level");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<Guid>("GrantedByUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("granted_by_user_id");

                    b.Property<DateTimeOffset?>("RevokedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("revoked_at");

                    b.HasKey("SessionId", "ActorUserId");

                    b.HasIndex("ActorUserId");

                    b.HasIndex("GrantedByUserId");

                    b.ToTable("session_access_overrides", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.SessionKeyBlob", b =>
                {
                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_id");

                    b.Property<string>("RecipientDeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("recipient_device_id");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<byte[]>("EncryptedSessionKey")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("encrypted_session_key");

                    b.Property<long>("IssuedAtMs")
                        .HasColumnType("bigint")
                        .HasColumnName("issued_at_ms");

                    b.Property<int>("KeyGeneration")
                        .HasColumnType("integer")
                        .HasColumnName("key_generation");

                    b.Property<string>("SenderDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("sender_device_id");

                    b.Property<byte[]>("SenderKemPublicKey")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("sender_kem_public_key");

                    b.Property<byte[]>("Signature")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("signature");

                    b.HasKey("SessionId", "RecipientDeviceId");

                    b.HasIndex("SessionId");

                    b.ToTable("session_key_blobs", null, t =>
                        {
                            t.HasCheckConstraint("CK_session_key_blobs_issued_at_ms_positive", "\"issued_at_ms\" > 0");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.User", b =>
                {
                    b.Property<Guid>("Id")
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<string>("AuthSubject")
                        .IsRequired()
                        .HasColumnType("text")
                        .HasColumnName("auth_subject");

                    b.Property<string>("AvatarUrl")
                        .HasMaxLength(2048)
                        .HasColumnType("character varying(2048)")
                        .HasColumnName("avatar_url");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<string>("DisplayName")
                        .IsRequired()
                        .HasMaxLength(128)
                        .HasColumnType("character varying(128)")
                        .HasColumnName("display_name");

                    b.Property<string>("Email")
                        .IsRequired()
                        .HasMaxLength(320)
                        .HasColumnType("character varying(320)")
                        .HasColumnName("email");

                    b.Property<string>("Handle")
                        .IsRequired()
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("handle");

                    b.HasKey("Id");

                    b.HasIndex("AuthSubject")
                        .IsUnique();

                    b.HasIndex("Handle")
                        .IsUnique();

                    b.ToTable("users", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.UserDevice", b =>
                {
                    b.Property<Guid>("UserId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_id");

                    b.Property<string>("DeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("device_id");

                    b.Property<DateTimeOffset?>("CertExpiresAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("cert_expires_at");

                    b.Property<DateTimeOffset?>("CertIssuedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("cert_issued_at");

                    b.Property<string>("CertSignerDeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("cert_signer_device_id");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<DateTimeOffset?>("DeletedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("deleted_at");

                    b.Property<byte[]>("DeviceCertificate")
                        .HasColumnType("bytea")
                        .HasColumnName("device_certificate");

                    b.Property<byte[]>("DeviceCertificateSignature")
                        .HasColumnType("bytea")
                        .HasColumnName("device_certificate_signature");

                    b.Property<string>("DeviceLabel")
                        .HasMaxLength(128)
                        .HasColumnType("character varying(128)")
                        .HasColumnName("device_label");

                    b.Property<byte[]>("KemPublicKey")
                        .IsRequired()
                        .HasMaxLength(1184)
                        .HasColumnType("bytea")
                        .HasColumnName("kem_public_key");

                    b.Property<DateTimeOffset?>("RevokedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("revoked_at");

                    b.Property<string>("RevokedByDeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("revoked_by_device_id");

                    b.Property<byte[]>("SigningPublicKey")
                        .IsRequired()
                        .HasMaxLength(1952)
                        .HasColumnType("bytea")
                        .HasColumnName("signing_public_key");

                    b.HasKey("UserId", "DeviceId");

                    b.HasIndex("DeviceId")
                        .IsUnique();

                    b.HasIndex("UserId");

                    b.HasIndex("UserId", "RevokedAt", "DeletedAt");

                    b.ToTable("user_devices", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.UserDeviceList", b =>
                {
                    b.Property<Guid>("UserId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_id");

                    b.Property<long>("Generation")
                        .HasColumnType("bigint")
                        .HasColumnName("generation");

                    b.Property<byte[]>("Body")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("body");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<string>("DeviceIdsJson")
                        .IsRequired()
                        .HasColumnType("jsonb")
                        .HasColumnName("device_ids");

                    b.Property<long?>("ExpiresAtMs")
                        .HasColumnType("bigint")
                        .HasColumnName("expires_at_ms");

                    b.Property<long>("IssuedAtMs")
                        .HasColumnType("bigint")
                        .HasColumnName("issued_at_ms");

                    b.Property<byte[]>("Signature")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("signature");

                    b.Property<string>("SignerDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("signer_device_id");

                    b.HasKey("UserId", "Generation");

                    b.ToTable("user_device_lists", null, t =>
                        {
                            t.HasCheckConstraint("CK_user_device_lists_device_ids_bounded", "length(\"device_ids\"::text) < 65536");

                            t.HasCheckConstraint("CK_user_device_lists_generation_positive", "\"generation\" >= 1");

                            t.HasCheckConstraint("CK_user_device_lists_issued_at_positive", "\"issued_at_ms\" > 0");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.Workspace", b =>
                {
                    b.Property<Guid>("Id")
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<string>("Name")
                        .IsRequired()
                        .HasMaxLength(128)
                        .HasColumnType("character varying(128)")
                        .HasColumnName("name");

                    b.Property<Guid>("OwnerUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("owner_user_id");

                    b.Property<string>("Slug")
                        .IsRequired()
                        .HasMaxLength(128)
                        .HasColumnType("character varying(128)")
                        .HasColumnName("slug");

                    b.HasKey("Id");

                    b.HasIndex("OwnerUserId");

                    b.HasIndex("Slug")
                        .IsUnique();

                    b.ToTable("workspaces", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.WorkspaceMember", b =>
                {
                    b.Property<Guid>("WorkspaceId")
                        .HasColumnType("uuid")
                        .HasColumnName("workspace_id");

                    b.Property<Guid>("UserId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_id");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<DateTimeOffset?>("RevokedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("revoked_at");

                    b.Property<string>("Role")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("role");

                    b.HasKey("WorkspaceId", "UserId");

                    b.HasIndex("UserId", "RevokedAt");

                    b.ToTable("workspace_members", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.WorkspaceMemberAuditEntry", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<string>("Action")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("action");

                    b.Property<Guid>("ActorUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("actor_user_id");

                    b.Property<string>("ClientIp")
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("client_ip");

                    b.Property<DateTimeOffset>("OccurredAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("occurred_at");

                    b.Property<Guid>("TargetUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("target_user_id");

                    b.Property<string>("UserAgent")
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("user_agent");

                    b.Property<Guid>("WorkspaceId")
                        .HasColumnType("uuid")
                        .HasColumnName("workspace_id");

                    b.HasKey("Id");

                    b.HasIndex("ActorUserId");

                    b.HasIndex("OccurredAt");

                    b.HasIndex("TargetUserId");

                    b.HasIndex("WorkspaceId");

                    b.ToTable("workspace_member_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.DeviceLinkRequest", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("UserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.DeviceRegistrationChallenge", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("UserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.ExternalIdentity", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("UserId")
                        .OnDelete(DeleteBehavior.Cascade)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.Friendship", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("RequestorUserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("UserHighId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("UserLowId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.InputAuditEntry", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("SenderUserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.Session", null)
                        .WithMany()
                        .HasForeignKey("SessionId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.Session", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("OwnerUserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.Workspace", null)
                        .WithMany()
                        .HasForeignKey("WorkspaceId")
                        .OnDelete(DeleteBehavior.Restrict);
                });

            modelBuilder.Entity("Kodosi.Domain.SessionAccessOverride", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("ActorUserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("GrantedByUserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.Session", null)
                        .WithMany()
                        .HasForeignKey("SessionId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.SessionKeyBlob", b =>
                {
                    b.HasOne("Kodosi.Domain.Session", null)
                        .WithMany()
                        .HasForeignKey("SessionId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.UserDevice", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("UserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.UserDeviceList", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("UserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.Workspace", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("OwnerUserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.WorkspaceMember", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("UserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.Workspace", null)
                        .WithMany()
                        .HasForeignKey("WorkspaceId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });
#pragma warning restore 612, 618
        }
    }
}
