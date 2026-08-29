using Kodosi.Infrastructure.Persistence;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Infrastructure;
using Microsoft.EntityFrameworkCore.Metadata;
using Microsoft.EntityFrameworkCore.Migrations;
using Microsoft.EntityFrameworkCore.Migrations.Operations;

namespace Kodosi.HostTests;

public sealed class MigrationLineageTests
{
    private const string KnownDefectMigrationId =
        "20260817112447_DropUnusedPersistenceIndexes";

    [Fact]
    public void Successive_Target_Models_Reflect_Table_Column_And_Index_Operations()
    {
        using var context = CreateContext();
        var migrationsAssembly = context.GetService<IMigrationsAssembly>();
        var initializer = context.GetService<IModelRuntimeInitializer>();
        var migrations = migrationsAssembly.Migrations
            .Select(pair => (
                Id: pair.Key,
                Migration: migrationsAssembly.CreateMigration(
                    pair.Value,
                    context.Database.ProviderName!)))
            .ToArray();
        var targetModels = migrations.ToDictionary(
            pair => pair.Id,
            pair => initializer.Initialize(pair.Migration.TargetModel, designTime: true)
                .GetRelationalModel(),
            StringComparer.Ordinal);
        var mappedTables = targetModels.Values
            .SelectMany(model => model.Tables)
            .Select(table => new TableId(table.Schema, table.Name))
            .ToHashSet();
        var actual = new SchemaShape(mappedTables);

        foreach (var (migrationId, migration) in migrations)
        {
            actual.Apply(migration.UpOperations);
            var expected = SchemaShape.From(targetModels[migrationId], mappedTables);

            if (migrationId == KnownDefectMigrationId)
            {
                AssertKnownDefect(migrationId, actual, expected);
            }
            else
            {
                AssertShapeEqual(migrationId, expected, actual);
            }
        }
    }

    private static void AssertKnownDefect(
        string migrationId,
        SchemaShape actual,
        SchemaShape expected)
    {
        var defect = new ColumnId(
            new TableId(null, "session_viewer_dismissals"),
            "created_at");
        Assert.DoesNotContain(defect, actual.Columns);
        Assert.True(expected.Columns.Remove(defect));
        AssertShapeEqual(migrationId, expected, actual);
    }

    private static void AssertShapeEqual(
        string migrationId,
        SchemaShape expected,
        SchemaShape actual)
    {
        AssertSetEqual(migrationId, "tables", expected.Tables, actual.Tables);
        AssertSetEqual(migrationId, "columns", expected.Columns, actual.Columns);
        AssertSetEqual(migrationId, "indexes", expected.Indexes, actual.Indexes);
    }

    private static void AssertSetEqual<T>(
        string migrationId,
        string elementKind,
        HashSet<T> expected,
        HashSet<T> actual)
    {
        var missing = expected.Except(actual).ToArray();
        var unexpected = actual.Except(expected).ToArray();
        Assert.True(
            missing.Length == 0 && unexpected.Length == 0,
            $"{migrationId} {elementKind}: missing [{string.Join(", ", missing)}]; "
                + $"unexpected [{string.Join(", ", unexpected)}]");
    }

    private static KodosiDbContext CreateContext()
    {
        var options = new DbContextOptionsBuilder<KodosiDbContext>()
            .UseNpgsql("Host=localhost;Database=kodosi_migration_lineage")
            .Options;
        return new KodosiDbContext(options);
    }

    private sealed class SchemaShape(HashSet<TableId> mappedTables)
    {
        public HashSet<TableId> Tables { get; } = [];
        public HashSet<ColumnId> Columns { get; } = [];
        public HashSet<IndexId> Indexes { get; } = [];

        public static SchemaShape From(IRelationalModel model, HashSet<TableId> mappedTables)
        {
            var shape = new SchemaShape(mappedTables);
            foreach (var table in model.Tables)
            {
                var tableId = new TableId(table.Schema, table.Name);
                shape.Tables.Add(tableId);
                shape.Columns.UnionWith(
                    table.Columns.Select(column => new ColumnId(tableId, column.Name)));
                shape.Indexes.UnionWith(
                    table.Indexes.Select(index => new IndexId(tableId, index.Name)));
            }

            return shape;
        }

        public void Apply(IEnumerable<MigrationOperation> operations)
        {
            foreach (var operation in operations)
            {
                switch (operation)
                {
                    case CreateTableOperation create:
                        AddTable(
                            new TableId(create.Schema, create.Name),
                            create.Columns.Select(column => column.Name));
                        break;
                    case DropTableOperation drop:
                        RemoveTable(new TableId(drop.Schema, drop.Name));
                        break;
                    case RenameTableOperation rename:
                        RenameTable(
                            new TableId(rename.Schema, rename.Name),
                            new TableId(rename.NewSchema ?? rename.Schema, rename.NewName ?? rename.Name));
                        break;
                    case AddColumnOperation add when IsMapped(add.Schema, add.Table):
                        Columns.Add(new ColumnId(new TableId(add.Schema, add.Table), add.Name));
                        break;
                    case DropColumnOperation drop when IsMapped(drop.Schema, drop.Table):
                        Columns.Remove(new ColumnId(new TableId(drop.Schema, drop.Table), drop.Name));
                        break;
                    case RenameColumnOperation rename when IsMapped(rename.Schema, rename.Table):
                        var table = new TableId(rename.Schema, rename.Table);
                        Columns.Remove(new ColumnId(table, rename.Name));
                        Columns.Add(new ColumnId(table, rename.NewName));
                        break;
                    case CreateIndexOperation create when IsMapped(create.Schema, create.Table):
                        Indexes.Add(new IndexId(
                            new TableId(create.Schema, create.Table),
                            create.Name));
                        break;
                    case DropIndexOperation drop:
                        RemoveIndex(drop.Schema, drop.Table, drop.Name);
                        break;
                    case RenameIndexOperation rename:
                        RenameIndex(rename.Schema, rename.Table, rename.Name, rename.NewName);
                        break;
                }
            }
        }

        private bool IsMapped(string? schema, string table)
            => mappedTables.Contains(new TableId(schema, table));

        private void AddTable(TableId table, IEnumerable<string> columns)
        {
            if (!mappedTables.Contains(table))
            {
                return;
            }

            Tables.Add(table);
            Columns.UnionWith(columns.Select(column => new ColumnId(table, column)));
        }

        private void RemoveTable(TableId table)
        {
            Tables.Remove(table);
            Columns.RemoveWhere(column => column.Table == table);
            Indexes.RemoveWhere(index => index.Table == table);
        }

        private void RenameTable(TableId oldTable, TableId newTable)
        {
            var columns = Columns
                .Where(column => column.Table == oldTable)
                .Select(column => column.Name)
                .ToArray();
            var indexes = Indexes
                .Where(index => index.Table == oldTable)
                .Select(index => index.Name)
                .ToArray();
            RemoveTable(oldTable);
            AddTable(newTable, columns);
            Indexes.UnionWith(indexes.Select(index => new IndexId(newTable, index)));
        }

        private void RemoveIndex(string? schema, string? table, string name)
        {
            if (table is not null)
            {
                Indexes.Remove(new IndexId(new TableId(schema, table), name));
                return;
            }

            Indexes.RemoveWhere(index => index.Table.Schema == schema && index.Name == name);
        }

        private void RenameIndex(
            string? schema,
            string? table,
            string name,
            string newName)
        {
            var oldIndex = table is null
                ? Indexes.Single(index => index.Table.Schema == schema && index.Name == name)
                : new IndexId(new TableId(schema, table), name);
            Indexes.Remove(oldIndex);
            Indexes.Add(new IndexId(oldIndex.Table, newName));
        }
    }

    private sealed record TableId(string? Schema, string Name);
    private sealed record ColumnId(TableId Table, string Name);
    private sealed record IndexId(TableId Table, string Name);
}
