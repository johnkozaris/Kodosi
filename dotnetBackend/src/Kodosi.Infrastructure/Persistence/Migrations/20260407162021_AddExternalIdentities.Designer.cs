
using System;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Infrastructure;
using Microsoft.EntityFrameworkCore.Migrations;
using Microsoft.EntityFrameworkCore.Storage.ValueConversion;
using Npgsql.EntityFrameworkCore.PostgreSQL.Metadata;
using Kodosi.Infrastructure.Persistence;

#nullable disable

namespace Kodosi.Infrastructure.Persistence.Migrations
{
    [DbContext(typeof(KodosiDbContext))]
    [Migration("20260407162021_AddExternalIdentities")]
    partial class AddExternalIdentities
    {
                protected override void BuildTargetModel(ModelBuilder modelBuilder)
        {
#pragma warning disable 612, 618
            modelBuilder
                .HasAnnotation("ProductVersion", "10.0.5")
                .HasAnnotation("Relational:MaxIdentifierLength", 63);

            NpgsqlModelBuilderExtensions.UseIdentityByDefaultColumns(modelBuilder);

            modelBuilder.Entity("Kodosi.Domain.Session", b =>
                {
                    b.Property<Guid>("Id")
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<string>("DefaultAccess")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("default_access");

                    b.Property<DateTimeOffset?>("EndedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("ended_at");

                    b.Property<DateTimeOffset>("LastHeartbeatAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("last_heartbeat_at");

                    b.Property<string>("OwnerSessionSecretHash")
                        .IsRequired()
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("owner_coding_session_secret_hash");

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

                    b.ToTable("coding_sessions", null, t =>
                        {
                            t.HasCheckConstraint("CK_coding_sessions_public_scope_is_view_only", "\"scope\" <> 'Public' OR \"default_access\" = 'View'");

                            t.HasCheckConstraint("CK_coding_sessions_workspace_scope_matches_workspace_id", "(\"scope\" = 'Workspace' AND \"workspace_id\" IS NOT NULL)\nOR (\"scope\" <> 'Workspace' AND \"workspace_id\" IS NULL)");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.SessionAccessOverride", b =>
                {
                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("coding_session_id");

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

                    b.ToTable("coding_session_access_overrides", (string)null);
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

                    b.HasKey("UserLowId", "UserHighId");

                    b.HasIndex("RequestorUserId");

                    b.HasIndex("UserHighId");

                    b.HasIndex("Status", "RequestorUserId");

                    b.HasIndex("Status", "UserHighId");

                    b.HasIndex("Status", "UserLowId");

                    b.ToTable("friendships", (string)null);
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

                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("coding_session_id");

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

                    b.Property<string>("PayloadPreview")
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("payload_preview");

                    b.Property<string>("PayloadSha256")
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("payload_sha256");

                    b.Property<Guid>("SenderUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("sender_user_id");

                    b.Property<string>("Status")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("status");

                    b.Property<DateTimeOffset>("SubmittedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("submitted_at");

                    b.HasKey("Id");

                    b.HasIndex("SessionId");

                    b.HasIndex("SenderUserId");

                    b.HasIndex("SessionId", "SenderUserId", "ClientCommandId")
                        .IsUnique();

                    b.ToTable("input_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.SessionKeyBlob", b =>
                {
                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("coding_session_id");

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

                    b.Property<byte[]>("SenderSigningPublicKey")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("sender_signing_public_key");

                    b.Property<byte[]>("Signature")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("signature");

                    b.HasKey("SessionId", "RecipientDeviceId");

                    b.HasIndex("SessionId");

                    b.ToTable("session_key_blobs", (string)null);
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

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

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

                    b.HasKey("UserId", "DeviceId");

                    b.HasIndex("DeviceId")
                        .IsUnique();

                    b.HasIndex("UserId");

                    b.ToTable("user_devices", (string)null);
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

                    b.HasOne("Kodosi.Domain.Session", null)
                        .WithMany()
                        .HasForeignKey("SessionId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("GrantedByUserId")
                        .OnDelete(DeleteBehavior.Restrict)
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
                    b.HasOne("Kodosi.Domain.Session", null)
                        .WithMany()
                        .HasForeignKey("SessionId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("SenderUserId")
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

            modelBuilder.Entity("Kodosi.Domain.ExternalIdentity", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("UserId")
                        .OnDelete(DeleteBehavior.Cascade)
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
