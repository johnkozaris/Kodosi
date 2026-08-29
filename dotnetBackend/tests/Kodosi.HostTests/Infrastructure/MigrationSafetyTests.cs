using System.Reflection;
using Kodosi.Domain;
using Kodosi.Infrastructure.Persistence;
using Kodosi.Infrastructure.Persistence.Migrations;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Infrastructure;
using Microsoft.EntityFrameworkCore.Metadata;
using Microsoft.EntityFrameworkCore.Migrations;
using Microsoft.EntityFrameworkCore.Migrations.Operations;

namespace Kodosi.HostTests;

public sealed class MigrationSafetyTests
{
    [Theory]
    [InlineData(typeof(EncryptRoomContent))]
    [InlineData(typeof(AddSignedRoomRoster))]
    public void Historical_Room_Migrations_Fail_Closed_Instead_Of_Deleting(
        Type migrationType)
    {
        var migration = (Migration)Activator.CreateInstance(migrationType)!;
        var builder = new MigrationBuilder("Npgsql.EntityFrameworkCore.PostgreSQL");
        migrationType
            .GetMethod("Up", BindingFlags.Instance | BindingFlags.NonPublic)!
            .Invoke(migration, [builder]);

        var sql = string.Join(
            '\n',
            builder.Operations.OfType<SqlOperation>().Select(operation => operation.Sql));

        Assert.Contains("RAISE EXCEPTION", sql, StringComparison.Ordinal);
        Assert.DoesNotContain("DELETE FROM", sql, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void CollapseUserDeviceCertificateAuthority_PreflightsAndRefusesFabricatedRollback()
    {
        var up = GetOperations<CollapseUserDeviceCertificateAuthority>("Up");
        var preflight = Assert.IsType<SqlOperation>(up[0]).Sql;
        Assert.Contains("body_length NOT BETWEEN 1 AND 5772", preflight, StringComparison.Ordinal);
        Assert.Contains("octet_length(device.device_certificate_signature) <> 3309", preflight, StringComparison.Ordinal);
        Assert.Contains("certificate_user_id IS DISTINCT FROM device.user_id::TEXT", preflight, StringComparison.Ordinal);
        Assert.Contains("certificate_device_id IS DISTINCT FROM device.device_id", preflight, StringComparison.Ordinal);
        Assert.Contains("certificate_label IS DISTINCT FROM device.device_label", preflight, StringComparison.Ordinal);
        Assert.Contains("certificate_signer_id IS DISTINCT FROM device.cert_signer_device_id", preflight, StringComparison.Ordinal);
        Assert.Contains("certificate_kem_key IS DISTINCT FROM device.kem_public_key", preflight, StringComparison.Ordinal);
        Assert.Contains("certificate_signing_key IS DISTINCT FROM device.signing_public_key", preflight, StringComparison.Ordinal);
        Assert.Contains("certificate timestamps are truncated or trailing bytes are present", preflight, StringComparison.Ordinal);
        Assert.Contains("extract(epoch FROM device.cert_issued_at) * 1000", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
        var listPreflight = Assert.IsType<SqlOperation>(up[1]).Sql;
        Assert.Contains("octet_length(body) NOT BETWEEN 1 AND 527432", listPreflight, StringComparison.Ordinal);
        Assert.Contains("octet_length(signature) <> 3309", listPreflight, StringComparison.Ordinal);
        Assert.Contains(
            up,
            operation => operation is AddCheckConstraintOperation
            {
                Name: "CK_user_device_lists_signature_exact",
            });
        Assert.Contains(
            up,
            operation => operation is DropColumnOperation { Table: "user_devices", Name: "kem_public_key" });
        Assert.Contains(
            up,
            operation => operation is AddCheckConstraintOperation { Name: "CK_user_devices_certificate_signature_exact" });

        var down = GetOperations<CollapseUserDeviceCertificateAuthority>("Down");
        var rollbackGuard = Assert.IsType<SqlOperation>(down[0]).Sql;
        Assert.Contains("IF EXISTS (SELECT 1 FROM user_devices)", rollbackGuard, StringComparison.Ordinal);
        Assert.Contains("without fabricating signed semantics", rollbackGuard, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", rollbackGuard, StringComparison.Ordinal);
    }

    [Fact]
    public void EnforceUserDeviceListProofPayload_PreflightsThenDropsLegacyDefault()
    {
        var operations = GetOperations<EnforceUserDeviceListProofPayload>("Up");

        var preflight = Assert.IsType<SqlOperation>(operations[0]).Sql;
        Assert.Contains("octet_length(body) = 0", preflight, StringComparison.Ordinal);
        Assert.Contains("octet_length(signature) = 0", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);

        var dropDefault = Assert.IsType<SqlOperation>(operations[1]).Sql;
        Assert.Contains("ALTER COLUMN body DROP DEFAULT", dropDefault, StringComparison.Ordinal);
        Assert.Equal(
            "CK_user_device_lists_body_nonempty",
            Assert.IsType<AddCheckConstraintOperation>(operations[2]).Name);
        Assert.Equal(
            "CK_user_device_lists_signature_nonempty",
            Assert.IsType<AddCheckConstraintOperation>(operations[3]).Name);
    }

    [Fact]
    public void UserDeviceListModelRequiresCanonicalProofBounds()
    {
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql("Host=localhost;Database=kodosi_model")
            .Options;
        using var context = new KodosiDbContext(options);
        var entityType = context.GetService<IDesignTimeModel>()
            .Model
            .FindEntityType(typeof(UserDeviceList))!;

        Assert.Equal(
            "octet_length(\"body\") > 0",
            entityType.FindCheckConstraint("CK_user_device_lists_body_nonempty")!.Sql);
        Assert.Equal(
            "octet_length(\"signature\") = 3309",
            entityType.FindCheckConstraint("CK_user_device_lists_signature_exact")!.Sql);
        Assert.Equal(
            "octet_length(\"body\") <= 527432",
            entityType.FindCheckConstraint("CK_user_device_lists_body_bounded")!.Sql);
    }

    [Fact]
    public void AlignRoomSlugMaxLength_PreflightsExistingLongSlugsBeforeNarrowing()
    {
        var operations = GetOperations<AlignRoomSlugMaxLength>("Up");

        var preflight = Assert.IsType<SqlOperation>(operations[0]).Sql;
        Assert.Contains("char_length(slug) > 64", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
        Assert.Contains("Rename every room", preflight, StringComparison.Ordinal);
        var alter = Assert.IsType<AlterColumnOperation>(operations[1]);
        Assert.Equal("rooms", alter.Table);
        Assert.Equal("slug", alter.Name);
        Assert.Equal(64, alter.MaxLength);
    }

    [Fact]
    public void EncryptRoomContent_RollbackPreflightsBothLegacyLimitsBeforeSchemaChanges()
    {
        var operations = GetOperations<EncryptRoomContent>("Down");

        var preflight = Assert.IsType<SqlOperation>(operations[0]).Sql;
        Assert.Contains("char_length(title) > 200", preflight, StringComparison.Ordinal);
        Assert.Contains("char_length(body) > 4000", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
        Assert.IsType<DropColumnOperation>(operations[1]);
    }

    [Fact]
    public void AddRoomChatAuthorKind_Preserves_Historical_Backfill_Default()
    {
        var operations = GetOperations<AddRoomChatAuthorKind>("Up");

        var add = Assert.IsType<AddColumnOperation>(Assert.Single(operations));
        Assert.Equal("Human", add.DefaultValue);
        Assert.IsType<DropColumnOperation>(
            Assert.Single(GetOperations<AddRoomChatAuthorKind>("Down")));
    }

    [Fact]
    public void DropRoomChatAuthorKindDefault_Removes_Backfill_Default()
    {
        var operations = GetOperations<DropRoomChatAuthorKindDefault>("Up");

        var dropDefault = Assert.IsType<SqlOperation>(Assert.Single(operations)).Sql;
        Assert.Contains("ALTER COLUMN author_kind DROP DEFAULT", dropDefault, StringComparison.Ordinal);
    }

    [Fact]
    public void RoomChatAuthorKind_ModelHasNoDatabaseDefault()
    {
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql("Host=localhost;Database=kodosi_model")
            .Options;
        using var context = new KodosiDbContext(options);
        var property = context.Model
            .FindEntityType(typeof(RoomChatMessage))!
            .FindProperty(nameof(RoomChatMessage.AuthorKind))!;

        Assert.Null(property.FindAnnotation(RelationalAnnotationNames.DefaultValue));
        Assert.Null(property.GetDefaultValueSql());
    }

    [Fact]
    public void DropRoomChatAuthorKindDefault_RollbackPreflightsAgentAttribution()
    {
        var operations = GetOperations<DropRoomChatAuthorKindDefault>("Down");

        var preflight = Assert.IsType<SqlOperation>(Assert.Single(operations)).Sql;
        Assert.Contains("author_kind = 'Agent'", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
        Assert.Contains("ALTER COLUMN author_kind SET DEFAULT 'Human'", preflight, StringComparison.Ordinal);
    }

    [Fact]
    public void AddRoomChatRecipients_RollbackPreflightsTargetedMessages()
    {
        var operations = GetOperations<AddRoomChatRecipients>("Down");

        var preflight = Assert.IsType<SqlOperation>(operations[0]).Sql;
        Assert.Contains("cardinality(recipient_session_ids) > 0", preflight, StringComparison.Ordinal);
        Assert.Contains("cardinality(recipient_user_ids) > 0", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
        Assert.IsType<DropColumnOperation>(operations[1]);
        Assert.IsType<DropColumnOperation>(operations[2]);
    }

    [Fact]
    public void AddSessionIncarnationId_Deletes_Only_Provably_Historical_Dismissals()
    {
        var operations = GetOperations<AddSessionIncarnationId>("Up");
        var delete = operations
            .OfType<SqlOperation>()
            .Single(operation => operation.Sql.Contains(
                "DELETE FROM session_viewer_dismissals",
                StringComparison.OrdinalIgnoreCase))
            .Sql;

        Assert.Contains(
            "USING sessions AS session",
            delete,
            StringComparison.OrdinalIgnoreCase);
        Assert.Contains(
            "dismissal.session_id = session.id",
            delete,
            StringComparison.OrdinalIgnoreCase);
        Assert.Contains(
            "dismissal.created_at < session.started_at",
            delete,
            StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void IdentityResetAudienceMigrationAddsNonNullUuidArrayWithEmptyBackfill()
    {
        var add = Assert.IsType<AddColumnOperation>(
            Assert.Single(GetOperations<AddIdentityResetAudience>("Up")));

        Assert.Equal("identity_reset_audit", add.Table);
        Assert.Equal("audience_user_ids", add.Name);
        Assert.Equal("uuid[]", add.ColumnType);
        Assert.False(add.IsNullable);
        Assert.Equal(Array.Empty<Guid>(), Assert.IsType<Guid[]>(add.DefaultValue));
    }

    [Fact]
    public void SemanticRelayMailboxRollbackRefusesToDiscardReceiptHistory()
    {
        var operations = GetOperations<AddSemanticRelayMailbox>("Down");

        var preflight = Assert.IsType<SqlOperation>(operations[0]).Sql;
        Assert.Contains("semantic_relay_requests", preflight, StringComparison.Ordinal);
        Assert.Contains("semantic_relay_receipts", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
        Assert.Contains("idempotency or receipt history", preflight, StringComparison.Ordinal);
        Assert.IsType<DropTableOperation>(operations[1]);
        Assert.IsType<DropTableOperation>(operations[2]);
    }

    [Fact]
    public void DeviceRevocationEnforcementRollbackRefusesPendingObligations()
    {
        var operations = GetOperations<AddDeviceRevocationEnforcement>("Down");

        var preflight = Assert.IsType<SqlOperation>(operations[0]).Sql;
        Assert.Contains("device_revocation_audit", preflight, StringComparison.Ordinal);
        Assert.Contains("realtime_enforced_at IS NULL", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
    }

    [Fact]
    public void DeviceRevocationSessionTargets_Retires_Legacy_Pending_Without_Reconstruction()
    {
        var operations = GetOperations<AddDeviceRevocationSessionTargets>("Up");

        var add = Assert.IsType<AddColumnOperation>(operations[0]);
        Assert.Equal("device_revocation_audit", add.Table);
        Assert.Equal("affected_session_targets", add.Name);
        Assert.Equal("jsonb", add.ColumnType);
        Assert.True(add.IsNullable);

        var retireLegacy = Assert.IsType<SqlOperation>(operations[1]).Sql;
        Assert.Contains("SET realtime_enforced_at = NOW()", retireLegacy, StringComparison.Ordinal);
        Assert.Contains("realtime_enforced_at IS NULL", retireLegacy, StringComparison.Ordinal);
        Assert.DoesNotContain("JOIN", retireLegacy, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("sessions", retireLegacy, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void DeviceRevocationSessionTargets_Rollback_Preserves_Exact_Audit_Evidence()
    {
        var operations = GetOperations<AddDeviceRevocationSessionTargets>("Down");

        var preflight = Assert.IsType<SqlOperation>(operations[0]).Sql;
        Assert.Contains("device_revocation_audit", preflight, StringComparison.Ordinal);
        Assert.Contains("affected_session_targets IS NOT NULL", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
        Assert.Contains("exact target audit evidence", preflight, StringComparison.Ordinal);
        var drop = Assert.IsType<DropColumnOperation>(operations[1]);
        Assert.Equal("device_revocation_audit", drop.Table);
        Assert.Equal("affected_session_targets", drop.Name);
    }

    [Fact]
    public void SessionEndReceiptRollbackRefusesToDiscardIdempotencyHistory()
    {
        var operations = GetOperations<AddSessionEndMutationReceipts>("Down");

        var preflight = Assert.IsType<SqlOperation>(operations[0]).Sql;
        Assert.Contains("session_end_mutations", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
        Assert.Contains("idempotency history", preflight, StringComparison.Ordinal);
        Assert.IsType<DropTableOperation>(operations[1]);
    }

    [Fact]
    public void RoomMutationReceiptMigrationBackfillsAssignedSessionIncarnationsBeforeConstraint()
    {
        var operations = GetOperations<AddRoomMutationReceipts>("Up");
        var backfillIndex = operations
            .Select((operation, index) => (operation, index))
            .Single(pair => pair.operation is SqlOperation sql
                && sql.Sql.Contains(
                    "SET assigned_session_incarnation_id = session.incarnation_id",
                    StringComparison.Ordinal))
            .index;
        var constraintIndex = operations
            .Select((operation, index) => (operation, index))
            .Single(pair => pair.operation is AddCheckConstraintOperation constraint
                && constraint.Name == "CK_room_tasks_assignee_incarnation_pair")
            .index;

        Assert.True(backfillIndex < constraintIndex);
    }

    [Fact]
    public void RoomMutationReceiptRollbackRefusesToDiscardIdempotencyHistory()
    {
        var operations = GetOperations<AddRoomMutationReceipts>("Down");

        var preflight = Assert.IsType<SqlOperation>(operations[0]).Sql;
        Assert.Contains("room_mutation_receipts", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
        Assert.Contains("idempotency history", preflight, StringComparison.Ordinal);
        Assert.IsType<DropTableOperation>(operations[1]);
    }

    [Fact]
    public void RepairCurrentKeyGenerationRaisesOnlyStaleSessionGenerations()
    {
        var up = Assert.IsType<SqlOperation>(
            Assert.Single(GetOperations<RepairCurrentKeyGeneration>("Up"))).Sql;

        Assert.Contains("UPDATE sessions AS session", up, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("coding_sessions", up, StringComparison.OrdinalIgnoreCase);
        Assert.Contains("MAX(key_generation)", up, StringComparison.OrdinalIgnoreCase);
        Assert.Contains("GROUP BY session_id", up, StringComparison.OrdinalIgnoreCase);
        Assert.Contains(
            "session.current_key_generation < latest.max_generation",
            up,
            StringComparison.OrdinalIgnoreCase);
        Assert.Empty(GetOperations<RepairCurrentKeyGeneration>("Down"));
    }

    [Fact]
    public void DropRedundantIdentityObservations_DropsOnlyDuplicateIdentityStorage()
    {
        var operations = GetOperations<DropRedundantIdentityObservations>("Up");

        var index = Assert.IsType<DropIndexOperation>(operations[0]);
        Assert.Equal("IX_users_auth_subject", index.Name);
        Assert.Equal("users", index.Table);

        var droppedColumns = operations
            .Skip(1)
            .Select(Assert.IsType<DropColumnOperation>)
            .Select(operation => (Table: operation.Table!, operation.Name))
            .ToHashSet();
        Assert.Equal(
            new HashSet<(string Table, string Name)>
            {
                ("users", "auth_subject"),
                ("external_identities", "avatar_url_snapshot"),
                ("external_identities", "display_name_snapshot"),
                ("external_identities", "email_snapshot"),
                ("external_identities", "email_verified"),
                ("external_identities", "last_seen_at"),
            },
            droppedColumns);
        Assert.Equal(7, operations.Count);
    }

    [Fact]
    public void DropRedundantIdentityObservations_RollbackRefusesToFabricateIdentityData()
    {
        var operations = GetOperations<DropRedundantIdentityObservations>("Down");

        var preflight = Assert.IsType<SqlOperation>(operations[0]).Sql;
        Assert.Contains("users", preflight, StringComparison.Ordinal);
        Assert.Contains("external_identities", preflight, StringComparison.Ordinal);
        Assert.Contains("RAISE EXCEPTION", preflight, StringComparison.Ordinal);
        Assert.Contains("cannot reconstruct", preflight, StringComparison.Ordinal);
        Assert.DoesNotContain(
            operations.Skip(1),
            operation => operation is SqlOperation);
    }

    [Fact]
    public void IdempotentScript_GuardsSlugPreflightWithMigrationHistoryCheck()
    {
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql("Host=localhost;Database=kodosi_migration_script")
            .Options;
        using var context = new KodosiDbContext(options);

        var script = context.GetService<IMigrator>().GenerateScript(
            fromMigration: "20260712044825_HardenAuditAndDeviceLinkConcurrency",
            toMigration: "20260712053948_AlignRoomSlugMaxLength",
            options: MigrationsSqlGenerationOptions.Idempotent);

        var historyGuard = script.IndexOf(
            "20260712053948_AlignRoomSlugMaxLength",
            StringComparison.Ordinal);
        var preflight = script.IndexOf("char_length(slug) > 64", StringComparison.Ordinal);
        var narrowing = script.IndexOf(
            "ALTER COLUMN slug TYPE character varying(64)",
            StringComparison.OrdinalIgnoreCase);

        Assert.True(historyGuard >= 0);
        Assert.True(preflight > historyGuard);
        Assert.True(narrowing > preflight);
        Assert.Contains("IF NOT EXISTS", script, StringComparison.OrdinalIgnoreCase);
    }

    private static IReadOnlyList<MigrationOperation> GetOperations<TMigration>(
        string methodName)
        where TMigration : Migration, new()
    {
        var migration = new TMigration();
        var builder = new MigrationBuilder("Npgsql.EntityFrameworkCore.PostgreSQL");
        typeof(TMigration)
            .GetMethod(methodName, BindingFlags.Instance | BindingFlags.NonPublic)!
            .Invoke(migration, [builder]);
        return builder.Operations;
    }
}
