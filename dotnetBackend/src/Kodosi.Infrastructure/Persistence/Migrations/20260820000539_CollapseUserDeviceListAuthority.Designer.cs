
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
    [Migration("20260820000539_CollapseUserDeviceListAuthority")]
    partial class CollapseUserDeviceListAuthority
    {
                protected override void BuildTargetModel(ModelBuilder modelBuilder)
        {
#pragma warning disable 612, 618
            modelBuilder
                .HasAnnotation("ProductVersion", "10.0.10")
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

                    b.ToTable("access_override_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.DeviceLinkRequest", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<DateTimeOffset?>("AcknowledgedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("acknowledged_at");

                    b.Property<DateTimeOffset?>("ApprovedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("approved_at");

                    b.Property<DateTimeOffset?>("CancelledAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("cancelled_at");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

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

                    b.Property<DateTimeOffset?>("ResultExpiresAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("result_expires_at");

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

                    b.Property<uint>("Version")
                        .IsConcurrencyToken()
                        .ValueGeneratedOnAddOrUpdate()
                        .HasColumnType("xid")
                        .HasColumnName("xmin");

                    b.HasKey("Id");

                    b.HasIndex("DeviceCode")
                        .IsUnique();

                    b.HasIndex("ExpiresAt");

                    b.HasIndex("ResultExpiresAt");

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

                    b.PrimitiveCollection<Guid[]>("AffectedSessionIds")
                        .IsRequired()
                        .HasColumnType("uuid[]")
                        .HasColumnName("affected_session_ids");

                    b.Property<string>("AffectedSessionTargetsJson")
                        .HasColumnType("jsonb")
                        .HasColumnName("affected_session_targets");

                    b.Property<int>("BlobsCascaded")
                        .HasColumnType("integer")
                        .HasColumnName("blobs_cascaded");

                    b.Property<string>("ClientIp")
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("client_ip");

                    b.Property<long>("IdentityRevision")
                        .HasColumnType("bigint")
                        .HasColumnName("identity_revision");

                    b.Property<int>("NewGeneration")
                        .HasColumnType("integer")
                        .HasColumnName("new_generation");

                    b.Property<DateTimeOffset>("OccurredAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("occurred_at");

                    b.Property<DateTimeOffset?>("RealtimeEnforcedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("realtime_enforced_at");

                    b.Property<Guid>("RevocationId")
                        .HasColumnType("uuid")
                        .HasColumnName("revocation_id");

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

                    b.HasIndex("OccurredAt");

                    b.HasIndex("RevocationId");

                    b.ToTable("device_revocation_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.ExternalIdentity", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<string>("Issuer")
                        .IsRequired()
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("issuer");

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

                    b.HasIndex("OtherUserId");

                    b.ToTable("friendship_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.IdentityExposure", b =>
                {
                    b.Property<Guid>("IdentityOwnerUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("identity_owner_user_id");

                    b.Property<Guid>("RecipientUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("recipient_user_id");

                    b.Property<DateTimeOffset>("FirstExposedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("first_exposed_at");

                    b.Property<DateTimeOffset>("LastExposedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("last_exposed_at");

                    b.HasKey("IdentityOwnerUserId", "RecipientUserId");

                    b.HasIndex("RecipientUserId");

                    b.ToTable("identity_exposures", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.IdentityResetAuditEntry", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.PrimitiveCollection<Guid[]>("AudienceUserIds")
                        .IsRequired()
                        .HasColumnType("uuid[]")
                        .HasColumnName("audience_user_ids");

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

                    b.PrimitiveCollection<Guid[]>("EndedSessionIds")
                        .IsRequired()
                        .HasColumnType("uuid[]")
                        .HasColumnName("ended_session_ids");

                    b.Property<string>("EndedSessionTargetsJson")
                        .HasColumnType("jsonb")
                        .HasColumnName("ended_session_targets");

                    b.Property<long>("IdentityRevision")
                        .HasColumnType("bigint")
                        .HasColumnName("identity_revision");

                    b.Property<DateTimeOffset?>("RealtimeEnforcedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("realtime_enforced_at");

                    b.PrimitiveCollection<string[]>("RemovedDeviceIds")
                        .IsRequired()
                        .HasColumnType("text[]")
                        .HasColumnName("removed_device_ids");

                    b.Property<DateTimeOffset>("ResetAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("reset_at");

                    b.Property<string>("SessionTargetsJson")
                        .HasColumnType("jsonb")
                        .HasColumnName("session_targets");

                    b.Property<int>("SessionsAttempted")
                        .HasColumnType("integer")
                        .HasColumnName("sessions_attempted");

                    b.Property<int>("SessionsEnded")
                        .HasColumnType("integer")
                        .HasColumnName("sessions_ended");

                    b.PrimitiveCollection<Guid[]>("SessionsWithRevokedKeys")
                        .IsRequired()
                        .HasColumnType("uuid[]")
                        .HasColumnName("sessions_with_revoked_keys");

                    b.Property<string>("UserAgent")
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("user_agent");

                    b.Property<Guid>("UserId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_id");

                    b.HasKey("Id");

                    b.HasIndex("RealtimeEnforcedAt");

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

                    b.Property<int>("DuplicateCount")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("integer")
                        .HasDefaultValue(0)
                        .HasColumnName("duplicate_count");

                    b.Property<string>("Kind")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("kind");

                    b.Property<DateTimeOffset?>("LastDuplicateAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("last_duplicate_at");

                    b.Property<int>("PayloadBytesLen")
                        .HasColumnType("integer")
                        .HasColumnName("payload_bytes_len");

                    b.Property<string>("PayloadSha256")
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("payload_sha256");

                    b.Property<long?>("PendingRequestGeneration")
                        .HasColumnType("bigint")
                        .HasColumnName("pending_request_generation");

                    b.Property<string>("PendingRequestId")
                        .HasMaxLength(128)
                        .HasColumnType("character varying(128)")
                        .HasColumnName("pending_request_id");

                    b.Property<string>("PendingRequesterDeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("pending_requester_device_id");

                    b.Property<long?>("PendingSessionIncarnationGeneration")
                        .HasColumnType("bigint")
                        .HasColumnName("pending_session_incarnation_generation");

                    b.Property<Guid?>("PendingSessionIncarnationId")
                        .HasColumnType("uuid")
                        .HasColumnName("pending_session_incarnation_id");

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

                    b.HasIndex("SessionId", "SenderUserId", "ClientCommandId")
                        .IsUnique();

                    b.ToTable("input_audit", null, t =>
                        {
                            t.HasCheckConstraint("ck_input_audit_permission_pending_tuple", "(pending_session_incarnation_id IS NULL AND pending_session_incarnation_generation IS NULL AND pending_request_id IS NULL AND pending_request_generation IS NULL AND pending_requester_device_id IS NULL) OR (pending_session_incarnation_id IS NOT NULL AND pending_session_incarnation_generation IS NOT NULL AND pending_session_incarnation_generation > 0 AND pending_request_id IS NOT NULL AND pending_request_generation IS NOT NULL AND pending_request_generation > 0 AND pending_requester_device_id IS NOT NULL)");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.Room", b =>
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

                    b.Property<byte[]>("RosterActivationDecisionBody")
                        .HasColumnType("bytea")
                        .HasColumnName("roster_activation_decision_body");

                    b.Property<byte[]>("RosterActivationDecisionSignature")
                        .HasColumnType("bytea")
                        .HasColumnName("roster_activation_decision_signature");

                    b.Property<string>("RosterActivationDecisionSignerDeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("roster_activation_decision_signer_device_id");

                    b.Property<DateTimeOffset?>("RosterActivationExpiresAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("roster_activation_expires_at");

                    b.Property<Guid?>("RosterActivationInvitationId")
                        .HasColumnType("uuid")
                        .HasColumnName("roster_activation_invitation_id");

                    b.Property<Guid?>("RosterActivationInviteeUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("roster_activation_invitee_user_id");

                    b.Property<byte[]>("RosterActivationProposalBody")
                        .HasColumnType("bytea")
                        .HasColumnName("roster_activation_proposal_body");

                    b.Property<byte[]>("RosterActivationProposalHash")
                        .HasColumnType("bytea")
                        .HasColumnName("roster_activation_proposal_hash");

                    b.Property<byte[]>("RosterActivationProposalSignature")
                        .HasColumnType("bytea")
                        .HasColumnName("roster_activation_proposal_signature");

                    b.Property<string>("RosterActivationProposalSignerDeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("roster_activation_proposal_signer_device_id");

                    b.Property<byte[]>("RosterBody")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("roster_body");

                    b.Property<long>("RosterGeneration")
                        .HasColumnType("bigint")
                        .HasColumnName("roster_generation");

                    b.Property<byte[]>("RosterSignature")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("roster_signature");

                    b.Property<string>("RosterSignerDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("roster_signer_device_id");

                    b.Property<string>("Slug")
                        .IsRequired()
                        .HasMaxLength(64)
                        .HasColumnType("character varying(64)")
                        .HasColumnName("slug");

                    b.Property<uint>("Version")
                        .IsConcurrencyToken()
                        .ValueGeneratedOnAddOrUpdate()
                        .HasColumnType("xid")
                        .HasColumnName("xmin");

                    b.HasKey("Id");

                    b.HasIndex("OwnerUserId");

                    b.HasIndex("Slug")
                        .IsUnique();

                    b.ToTable("rooms", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.RoomChatMessage", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<string>("AuthorKind")
                        .IsRequired()
                        .HasMaxLength(16)
                        .HasColumnType("character varying(16)")
                        .HasColumnName("author_kind");

                    b.Property<Guid?>("AuthorSessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("author_session_id");

                    b.Property<Guid>("AuthorUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("author_user_id");

                    b.Property<string>("Body")
                        .IsRequired()
                        .HasColumnType("text")
                        .HasColumnName("body");

                    b.Property<DateTimeOffset>("PostedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("posted_at");

                    b.PrimitiveCollection<Guid[]>("RecipientSessionIds")
                        .IsRequired()
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid[]")
                        .HasColumnName("recipient_session_ids")
                        .HasDefaultValueSql("'{}'::uuid[]");

                    b.PrimitiveCollection<Guid[]>("RecipientUserIds")
                        .IsRequired()
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid[]")
                        .HasColumnName("recipient_user_ids")
                        .HasDefaultValueSql("'{}'::uuid[]");

                    b.Property<Guid>("RoomId")
                        .HasColumnType("uuid")
                        .HasColumnName("room_id");

                    b.Property<long>("Seq")
                        .HasColumnType("bigint")
                        .HasColumnName("seq");

                    b.HasKey("Id");

                    b.HasIndex("RoomId", "Seq")
                        .IsUnique();

                    b.ToTable("room_chat_messages", null, t =>
                        {
                            t.HasCheckConstraint("ck_room_chat_messages_body_length", "char_length(body) BETWEEN 1 AND 1048576");

                            t.HasCheckConstraint("ck_room_chat_messages_recipient_count", "cardinality(recipient_session_ids) + cardinality(recipient_user_ids) <= 32");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.RoomInvitation", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<long>("BaseRosterGeneration")
                        .HasColumnType("bigint")
                        .HasColumnName("base_roster_generation");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<byte[]>("DecisionBody")
                        .HasColumnType("bytea")
                        .HasColumnName("decision_body");

                    b.Property<DateTimeOffset?>("DecisionIssuedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("decision_issued_at");

                    b.Property<byte[]>("DecisionSignature")
                        .HasColumnType("bytea")
                        .HasColumnName("decision_signature");

                    b.Property<string>("DecisionSignerDeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("decision_signer_device_id");

                    b.Property<DateTimeOffset>("ExpiresAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("expires_at");

                    b.Property<Guid>("InvitedByUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("invited_by_user_id");

                    b.Property<Guid>("InviteeUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("invitee_user_id");

                    b.Property<byte[]>("ProposalBody")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("proposal_body");

                    b.Property<byte[]>("ProposalHash")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("proposal_hash");

                    b.Property<DateTimeOffset>("ProposalIssuedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("proposal_issued_at");

                    b.Property<byte[]>("ProposalSignature")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("proposal_signature");

                    b.Property<string>("ProposalSignerDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("proposal_signer_device_id");

                    b.Property<byte[]>("ProposedRosterBody")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("proposed_roster_body");

                    b.Property<long>("ProposedRosterGeneration")
                        .HasColumnType("bigint")
                        .HasColumnName("proposed_roster_generation");

                    b.Property<byte[]>("ProposedRosterSignature")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("proposed_roster_signature");

                    b.Property<string>("ProposedRosterSignerDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("proposed_roster_signer_device_id");

                    b.Property<DateTimeOffset?>("RespondedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("responded_at");

                    b.Property<Guid>("RoomId")
                        .HasColumnType("uuid")
                        .HasColumnName("room_id");

                    b.Property<string>("Status")
                        .IsRequired()
                        .HasMaxLength(16)
                        .HasColumnType("character varying(16)")
                        .HasColumnName("status");

                    b.Property<uint>("Version")
                        .IsConcurrencyToken()
                        .ValueGeneratedOnAddOrUpdate()
                        .HasColumnType("xid")
                        .HasColumnName("xmin");

                    b.HasKey("Id");

                    b.HasIndex("ExpiresAt");

                    b.HasIndex("InvitedByUserId");

                    b.HasIndex("InviteeUserId");

                    b.HasIndex("RoomId", "InviteeUserId")
                        .IsUnique()
                        .HasDatabaseName("UX_room_invitations_pending_room_invitee")
                        .HasFilter("\"status\" = 'Pending'");

                    b.ToTable("room_invitations", null, t =>
                        {
                            t.HasCheckConstraint("CK_room_invitations_status_allowed_values", "\"status\" IN ('Pending', 'Accepted', 'Declined', 'Cancelled', 'Expired', 'Superseded')");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.RoomMember", b =>
                {
                    b.Property<Guid>("RoomId")
                        .HasColumnType("uuid")
                        .HasColumnName("room_id");

                    b.Property<Guid>("UserId")
                        .HasColumnType("uuid")
                        .HasColumnName("user_id");

                    b.Property<byte[]>("AdmissionDecisionBody")
                        .HasColumnType("bytea")
                        .HasColumnName("admission_decision_body");

                    b.Property<byte[]>("AdmissionDecisionSignature")
                        .HasColumnType("bytea")
                        .HasColumnName("admission_decision_signature");

                    b.Property<string>("AdmissionDecisionSignerDeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("admission_decision_signer_device_id");

                    b.Property<DateTimeOffset?>("AdmissionExpiresAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("admission_expires_at");

                    b.Property<Guid?>("AdmissionInvitationId")
                        .HasColumnType("uuid")
                        .HasColumnName("admission_invitation_id");

                    b.Property<byte[]>("AdmissionProposalBody")
                        .HasColumnType("bytea")
                        .HasColumnName("admission_proposal_body");

                    b.Property<byte[]>("AdmissionProposalHash")
                        .HasColumnType("bytea")
                        .HasColumnName("admission_proposal_hash");

                    b.Property<byte[]>("AdmissionProposalSignature")
                        .HasColumnType("bytea")
                        .HasColumnName("admission_proposal_signature");

                    b.Property<string>("AdmissionProposalSignerDeviceId")
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("admission_proposal_signer_device_id");

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

                    b.Property<uint>("Version")
                        .IsConcurrencyToken()
                        .ValueGeneratedOnAddOrUpdate()
                        .HasColumnType("xid")
                        .HasColumnName("xmin");

                    b.HasKey("RoomId", "UserId");

                    b.HasIndex("AdmissionInvitationId")
                        .IsUnique()
                        .HasFilter("\"admission_invitation_id\" IS NOT NULL");

                    b.HasIndex("UserId", "RevokedAt");

                    b.ToTable("room_members", null, t =>
                        {
                            t.HasCheckConstraint("CK_room_members_active_admission_proof", "\"revoked_at\" IS NOT NULL\nOR \"role\" = 'Owner'\nOR (\n    \"admission_invitation_id\" IS NOT NULL\n    AND \"admission_proposal_body\" IS NOT NULL\n    AND \"admission_proposal_signature\" IS NOT NULL\n    AND \"admission_proposal_signer_device_id\" IS NOT NULL\n    AND \"admission_proposal_hash\" IS NOT NULL\n    AND \"admission_decision_body\" IS NOT NULL\n    AND \"admission_decision_signature\" IS NOT NULL\n    AND \"admission_decision_signer_device_id\" IS NOT NULL\n    AND \"admission_expires_at\" IS NOT NULL\n)");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.RoomMemberAuditEntry", b =>
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

                    b.Property<Guid>("RoomId")
                        .HasColumnType("uuid")
                        .HasColumnName("room_id");

                    b.Property<Guid>("TargetUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("target_user_id");

                    b.Property<string>("UserAgent")
                        .HasMaxLength(512)
                        .HasColumnType("character varying(512)")
                        .HasColumnName("user_agent");

                    b.HasKey("Id");

                    b.ToTable("room_member_audit", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.RoomMutationReceipt", b =>
                {
                    b.Property<Guid>("ActorUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("actor_user_id");

                    b.Property<string>("Operation")
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("operation");

                    b.Property<Guid>("RequestId")
                        .HasColumnType("uuid")
                        .HasColumnName("request_id");

                    b.Property<Guid?>("AssigneeSessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("assignee_session_id");

                    b.Property<Guid?>("AssigneeSessionIncarnationId")
                        .HasColumnType("uuid")
                        .HasColumnName("assignee_session_incarnation_id");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<Guid>("EntityId")
                        .HasColumnType("uuid")
                        .HasColumnName("entity_id");

                    b.Property<string>("Result")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("result");

                    b.Property<long?>("Revision")
                        .HasColumnType("bigint")
                        .HasColumnName("revision");

                    b.Property<Guid>("RoomId")
                        .HasColumnType("uuid")
                        .HasColumnName("room_id");

                    b.Property<byte[]>("TargetFingerprint")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("bytea")
                        .HasColumnName("target_fingerprint");

                    b.HasKey("ActorUserId", "Operation", "RequestId");

                    b.HasIndex("RoomId");

                    b.ToTable("room_mutation_receipts", null, t =>
                        {
                            t.HasCheckConstraint("CK_room_mutation_receipts_assignee_pair", "(assignee_session_id IS NULL) = (assignee_session_incarnation_id IS NULL)");

                            t.HasCheckConstraint("CK_room_mutation_receipts_fingerprint_length", "octet_length(target_fingerprint) = 32");

                            t.HasCheckConstraint("CK_room_mutation_receipts_operation_allowed_values", "\"operation\" IN ('AcceptInvitation', 'DeclineInvitation', 'CancelInvitation', 'RemoveMember', 'AssignTask', 'TransitionTask')");

                            t.HasCheckConstraint("CK_room_mutation_receipts_revision_nonnegative", "revision IS NULL OR revision >= 0");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.RoomMutationSessionEffect", b =>
                {
                    b.Property<Guid>("ActorUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("actor_user_id");

                    b.Property<string>("Operation")
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("operation");

                    b.Property<Guid>("RequestId")
                        .HasColumnType("uuid")
                        .HasColumnName("request_id");

                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_id");

                    b.Property<Guid>("SessionIncarnationId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_incarnation_id");

                    b.Property<bool>("EndedByRemoval")
                        .HasColumnType("boolean")
                        .HasColumnName("ended_by_removal");

                    b.Property<Guid>("OwnerUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("owner_user_id");

                    b.Property<DateTimeOffset>("StartedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("started_at");

                    b.HasKey("ActorUserId", "Operation", "RequestId", "SessionId", "SessionIncarnationId");

                    b.ToTable("room_mutation_session_effects", null, t =>
                        {
                            t.HasCheckConstraint("CK_room_mutation_session_effects_remove_member_only", "operation = 'RemoveMember'");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.RoomRosterTransition", b =>
                {
                    b.Property<Guid>("RoomId")
                        .HasColumnType("uuid")
                        .HasColumnName("room_id");

                    b.Property<long>("Generation")
                        .HasColumnType("bigint")
                        .HasColumnName("generation");

                    b.Property<Guid?>("AdmissionInvitationId")
                        .HasColumnType("uuid")
                        .HasColumnName("admission_invitation_id");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<byte[]>("RosterBody")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("roster_body");

                    b.Property<byte[]>("RosterSignature")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("roster_signature");

                    b.Property<string>("RosterSignerDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("roster_signer_device_id");

                    b.HasKey("RoomId", "Generation");

                    b.HasIndex("AdmissionInvitationId");

                    b.ToTable("room_roster_transitions", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.RoomTask", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<Guid?>("AssignedSessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("assigned_session_id");

                    b.Property<Guid?>("AssignedSessionIncarnationId")
                        .HasColumnType("uuid")
                        .HasColumnName("assigned_session_incarnation_id");

                    b.Property<DateTimeOffset?>("CompletedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("completed_at");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<Guid>("CreatedByUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("created_by_user_id");

                    b.Property<string>("Description")
                        .HasColumnType("text")
                        .HasColumnName("description");

                    b.Property<DateTimeOffset?>("DueAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("due_at");

                    b.Property<string>("Result")
                        .HasColumnType("text")
                        .HasColumnName("result");

                    b.Property<Guid?>("ResultAuthorUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("result_author_user_id");

                    b.Property<long>("Revision")
                        .HasColumnType("bigint")
                        .HasColumnName("revision");

                    b.Property<Guid>("RoomId")
                        .HasColumnType("uuid")
                        .HasColumnName("room_id");

                    b.Property<string>("Status")
                        .IsRequired()
                        .HasMaxLength(16)
                        .HasColumnType("character varying(16)")
                        .HasColumnName("status");

                    b.Property<string>("Title")
                        .IsRequired()
                        .HasColumnType("text")
                        .HasColumnName("title");

                    b.Property<DateTimeOffset>("UpdatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("updated_at");

                    b.Property<uint>("Version")
                        .IsConcurrencyToken()
                        .ValueGeneratedOnAddOrUpdate()
                        .HasColumnType("xid")
                        .HasColumnName("xmin");

                    b.HasKey("Id");

                    b.HasIndex("AssignedSessionId");

                    b.HasIndex("RoomId", "Status");

                    b.ToTable("room_tasks", null, t =>
                        {
                            t.HasCheckConstraint("CK_room_tasks_assignee_incarnation_pair", "(assigned_session_id IS NULL) = (assigned_session_incarnation_id IS NULL)");

                            t.HasCheckConstraint("CK_room_tasks_status_allowed_values", "\"status\" IN ('Open', 'InProgress', 'Review', 'Done', 'Archived')");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.SemanticRelayReceipt", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<DateTimeOffset?>("AcknowledgedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("acknowledged_at");

                    b.Property<Guid>("IncarnationId")
                        .HasColumnType("uuid")
                        .HasColumnName("incarnation_id");

                    b.Property<string>("Mode")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("mode");

                    b.Property<string>("Outcome")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("outcome");

                    b.Property<string>("OwnerDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("owner_device_id");

                    b.Property<Guid>("OwnerUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("owner_user_id");

                    b.Property<string>("PayloadSha256")
                        .IsRequired()
                        .HasMaxLength(64)
                        .HasColumnType("character(64)")
                        .HasColumnName("payload_sha256")
                        .IsFixedLength();

                    b.Property<Guid>("RequestId")
                        .HasColumnType("uuid")
                        .HasColumnName("request_id");

                    b.Property<Guid>("RequestRowId")
                        .HasColumnType("uuid")
                        .HasColumnName("request_row_id");

                    b.Property<string>("RequesterDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("requester_device_id");

                    b.Property<Guid>("RequesterUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("requester_user_id");

                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_id");

                    b.Property<string>("Signature")
                        .IsRequired()
                        .HasColumnType("text")
                        .HasColumnName("signature");

                    b.Property<DateTimeOffset>("StoredAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("stored_at");

                    b.HasKey("Id");

                    b.HasIndex("RequestRowId")
                        .IsUnique();

                    b.HasIndex("RequesterUserId", "AcknowledgedAt");

                    b.HasIndex("RequesterUserId", "RequestId")
                        .IsUnique();

                    b.ToTable("semantic_relay_receipts", null, t =>
                        {
                            t.HasCheckConstraint("CK_semantic_relay_receipts_mode", "mode IN ('queue', 'steer', 'stopAndSend')");

                            t.HasCheckConstraint("CK_semantic_relay_receipts_outcome", "outcome IN ('injected', 'cancelled', 'deliveryUnknown')");

                            t.HasCheckConstraint("CK_semantic_relay_receipts_owner_is_requester", "owner_user_id = requester_user_id");

                            t.HasCheckConstraint("CK_semantic_relay_receipts_payload_sha256", "payload_sha256 ~ '^[0-9a-f]{64}$'");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.SemanticRelayRequest", b =>
                {
                    b.Property<Guid>("Id")
                        .ValueGeneratedOnAdd()
                        .HasColumnType("uuid")
                        .HasColumnName("id");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<Guid>("IncarnationId")
                        .HasColumnType("uuid")
                        .HasColumnName("incarnation_id");

                    b.Property<string>("Mode")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("mode");

                    b.Property<string>("PayloadSha256")
                        .IsRequired()
                        .HasMaxLength(64)
                        .HasColumnType("character(64)")
                        .HasColumnName("payload_sha256")
                        .IsFixedLength();

                    b.Property<Guid>("RequestId")
                        .HasColumnType("uuid")
                        .HasColumnName("request_id");

                    b.Property<string>("RequesterDeviceId")
                        .IsRequired()
                        .HasMaxLength(256)
                        .HasColumnType("character varying(256)")
                        .HasColumnName("requester_device_id");

                    b.Property<Guid>("RequesterUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("requester_user_id");

                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_id");

                    b.Property<string>("State")
                        .IsRequired()
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("state");

                    b.Property<DateTimeOffset>("UpdatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("updated_at");

                    b.HasKey("Id");

                    b.HasIndex("RequesterUserId", "RequestId")
                        .IsUnique();

                    b.HasIndex("SessionId", "IncarnationId");

                    b.ToTable("semantic_relay_requests", null, t =>
                        {
                            t.HasCheckConstraint("CK_semantic_relay_requests_mode", "mode IN ('queue', 'steer', 'stopAndSend')");

                            t.HasCheckConstraint("CK_semantic_relay_requests_payload_sha256", "payload_sha256 ~ '^[0-9a-f]{64}$'");

                            t.HasCheckConstraint("CK_semantic_relay_requests_state", "state IN ('Pending', 'Dispatched', 'ReceiptStored', 'Acknowledged')");
                        });
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

                    b.Property<long>("IncarnationGeneration")
                        .HasColumnType("bigint")
                        .HasColumnName("incarnation_generation");

                    b.Property<Guid>("IncarnationId")
                        .HasColumnType("uuid")
                        .HasColumnName("incarnation_id");

                    b.Property<int>("IncarnationProtocolVersion")
                        .HasColumnType("integer")
                        .HasColumnName("incarnation_protocol_version");

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

                    b.Property<Guid?>("RoomId")
                        .HasColumnType("uuid")
                        .HasColumnName("room_id");

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

                    b.HasKey("Id");

                    b.HasIndex("OwnerUserId");

                    b.HasIndex("RoomId");

                    b.HasIndex("Status");

                    b.HasIndex("Scope", "Status");

                    b.HasIndex("OwnerUserId", "Status", "Scope");

                    b.ToTable("sessions", null, t =>
                        {
                            t.HasCheckConstraint("CK_sessions_room_scope_matches_room_id", "(\"scope\" = 'Room' AND \"room_id\" IS NOT NULL)\nOR (\"scope\" <> 'Room' AND \"room_id\" IS NULL)");

                            t.HasCheckConstraint("CK_sessions_scope_allowed_values", "\"scope\" IN ('JustMe', 'MyDevices', 'Friends', 'Room')");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.SessionAccessMutation", b =>
                {
                    b.Property<Guid>("RequesterUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("requester_user_id");

                    b.Property<Guid>("MutationId")
                        .HasColumnType("uuid")
                        .HasColumnName("mutation_id");

                    b.Property<string>("AccessLevel")
                        .HasMaxLength(32)
                        .HasColumnType("character varying(32)")
                        .HasColumnName("access_level");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<Guid>("IncarnationId")
                        .HasColumnType("uuid")
                        .HasColumnName("incarnation_id");

                    b.Property<string>("Kind")
                        .IsRequired()
                        .HasMaxLength(16)
                        .HasColumnType("character varying(16)")
                        .HasColumnName("kind");

                    b.Property<DateTimeOffset?>("RequestedExpiresAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("requested_expires_at");

                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_id");

                    b.Property<Guid?>("TargetUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("target_user_id");

                    b.HasKey("RequesterUserId", "MutationId");

                    b.ToTable("session_access_mutations", null, t =>
                        {
                            t.HasCheckConstraint("CK_session_access_mutations_access_level", "access_level IS NULL OR access_level IN ('View', 'Suggest', 'Inject', 'Approve')");

                            t.HasCheckConstraint("CK_session_access_mutations_kind", "kind IN ('Grant', 'Revoke', 'Leave')");

                            t.HasCheckConstraint("CK_session_access_mutations_shape", "(kind = 'Grant' AND target_user_id IS NOT NULL AND access_level IS NOT NULL AND requested_expires_at IS NOT NULL) OR (kind = 'Revoke' AND target_user_id IS NOT NULL AND access_level IS NULL AND requested_expires_at IS NULL) OR (kind = 'Leave' AND target_user_id IS NULL AND access_level IS NULL AND requested_expires_at IS NULL)");
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

                    b.Property<DateTimeOffset?>("ExpiresAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("expires_at");

                    b.Property<Guid>("GrantedByUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("granted_by_user_id");

                    b.Property<DateTimeOffset?>("RevokedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("revoked_at");

                    b.HasKey("SessionId", "ActorUserId");

                    b.HasIndex("ActorUserId");

                    b.HasIndex("ExpiresAt")
                        .HasFilter("\"revoked_at\" IS NULL AND \"expires_at\" IS NOT NULL");

                    b.HasIndex("GrantedByUserId");

                    b.ToTable("session_access_overrides", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.SessionEndMutation", b =>
                {
                    b.Property<Guid>("OwnerUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("owner_user_id");

                    b.Property<Guid>("MutationId")
                        .HasColumnType("uuid")
                        .HasColumnName("mutation_id");

                    b.Property<DateTimeOffset>("CreatedAt")
                        .HasColumnType("timestamp with time zone")
                        .HasColumnName("created_at");

                    b.Property<Guid>("FirstAttemptId")
                        .HasColumnType("uuid")
                        .HasColumnName("first_attempt_id");

                    b.Property<Guid>("IncarnationId")
                        .HasColumnType("uuid")
                        .HasColumnName("incarnation_id");

                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_id");

                    b.HasKey("OwnerUserId", "MutationId");

                    b.ToTable("session_end_mutations", (string)null);
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

                    b.Property<int>("SignatureVersion")
                        .HasColumnType("integer")
                        .HasColumnName("signature_version");

                    b.HasKey("SessionId", "RecipientDeviceId");

                    b.ToTable("session_key_blobs", null, t =>
                        {
                            t.HasCheckConstraint("CK_session_key_blobs_issued_at_ms_positive", "\"issued_at_ms\" > 0");
                        });
                });

            modelBuilder.Entity("Kodosi.Domain.SessionViewerDismissal", b =>
                {
                    b.Property<Guid>("SessionId")
                        .HasColumnType("uuid")
                        .HasColumnName("session_id");

                    b.Property<Guid>("ViewerUserId")
                        .HasColumnType("uuid")
                        .HasColumnName("viewer_user_id");

                    b.HasKey("SessionId", "ViewerUserId");

                    b.HasIndex("ViewerUserId");

                    b.ToTable("session_viewer_dismissals", (string)null);
                });

            modelBuilder.Entity("Kodosi.Domain.User", b =>
                {
                    b.Property<Guid>("Id")
                        .HasColumnType("uuid")
                        .HasColumnName("id");

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

                    b.Property<Guid?>("IdentityIncarnationId")
                        .HasColumnType("uuid")
                        .HasColumnName("identity_incarnation_id");

                    b.Property<long>("IdentityRevision")
                        .HasColumnType("bigint")
                        .HasColumnName("identity_revision");

                    b.HasKey("Id");

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

                    b.HasIndex("UserId", "RevokedAt");

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

                    b.Property<byte[]>("Signature")
                        .IsRequired()
                        .HasColumnType("bytea")
                        .HasColumnName("signature");

                    b.HasKey("UserId", "Generation");

                    b.ToTable("user_device_lists", null, t =>
                        {
                            t.HasCheckConstraint("CK_user_device_lists_body_bounded", "octet_length(\"body\") <= 33687588");

                            t.HasCheckConstraint("CK_user_device_lists_body_nonempty", "octet_length(\"body\") > 0");

                            t.HasCheckConstraint("CK_user_device_lists_generation_positive", "\"generation\" >= 1");

                            t.HasCheckConstraint("CK_user_device_lists_signature_nonempty", "octet_length(\"signature\") > 0");
                        });
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

            modelBuilder.Entity("Kodosi.Domain.Room", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("OwnerUserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.RoomChatMessage", b =>
                {
                    b.HasOne("Kodosi.Domain.Room", null)
                        .WithMany()
                        .HasForeignKey("RoomId")
                        .OnDelete(DeleteBehavior.Cascade)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.RoomInvitation", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("InviteeUserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.Room", null)
                        .WithMany()
                        .HasForeignKey("RoomId")
                        .OnDelete(DeleteBehavior.Cascade)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.RoomMember", b =>
                {
                    b.HasOne("Kodosi.Domain.Room", null)
                        .WithMany()
                        .HasForeignKey("RoomId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("UserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.RoomMutationReceipt", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("ActorUserId")
                        .OnDelete(DeleteBehavior.Cascade)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.Room", null)
                        .WithMany()
                        .HasForeignKey("RoomId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.RoomMutationSessionEffect", b =>
                {
                    b.HasOne("Kodosi.Domain.RoomMutationReceipt", null)
                        .WithMany()
                        .HasForeignKey("ActorUserId", "Operation", "RequestId")
                        .OnDelete(DeleteBehavior.Cascade)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.RoomRosterTransition", b =>
                {
                    b.HasOne("Kodosi.Domain.RoomInvitation", null)
                        .WithMany()
                        .HasForeignKey("AdmissionInvitationId")
                        .OnDelete(DeleteBehavior.Restrict);

                    b.HasOne("Kodosi.Domain.Room", null)
                        .WithMany()
                        .HasForeignKey("RoomId")
                        .OnDelete(DeleteBehavior.Cascade)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.RoomTask", b =>
                {
                    b.HasOne("Kodosi.Domain.Room", null)
                        .WithMany()
                        .HasForeignKey("RoomId")
                        .OnDelete(DeleteBehavior.Cascade)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.SemanticRelayReceipt", b =>
                {
                    b.HasOne("Kodosi.Domain.SemanticRelayRequest", null)
                        .WithOne()
                        .HasForeignKey("Kodosi.Domain.SemanticRelayReceipt", "RequestRowId")
                        .OnDelete(DeleteBehavior.Cascade)
                        .IsRequired();
                });

            modelBuilder.Entity("Kodosi.Domain.Session", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("OwnerUserId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.Room", null)
                        .WithMany()
                        .HasForeignKey("RoomId")
                        .OnDelete(DeleteBehavior.Restrict);
                });

            modelBuilder.Entity("Kodosi.Domain.SessionAccessMutation", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("RequesterUserId")
                        .OnDelete(DeleteBehavior.Cascade)
                        .IsRequired();
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

            modelBuilder.Entity("Kodosi.Domain.SessionEndMutation", b =>
                {
                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("OwnerUserId")
                        .OnDelete(DeleteBehavior.Cascade)
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

            modelBuilder.Entity("Kodosi.Domain.SessionViewerDismissal", b =>
                {
                    b.HasOne("Kodosi.Domain.Session", null)
                        .WithMany()
                        .HasForeignKey("SessionId")
                        .OnDelete(DeleteBehavior.Restrict)
                        .IsRequired();

                    b.HasOne("Kodosi.Domain.User", null)
                        .WithMany()
                        .HasForeignKey("ViewerUserId")
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
#pragma warning restore 612, 618
        }
    }
}
