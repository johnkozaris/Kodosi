using System.Globalization;
using System.Security.Cryptography;

namespace Kodosi.HostTests;

public sealed class HistoricalMigrationImmutabilityTests
{
    [Fact]
    public void Every_Migration_File_Matches_Its_Pinned_Bytes()
    {
        var directory = ResolveMigrationsDirectory();
        var pinned = ReadManifest();

        foreach (var (fileName, expected) in pinned)
        {
            var path = Path.Combine(directory, fileName);
            Assert.True(File.Exists(path), $"Pinned migration {fileName} was deleted.");

            var bytes = File.ReadAllBytes(path);
            Assert.Equal(
                (expected.Length, expected.Sha256),
                (bytes.Length, Convert.ToHexStringLower(SHA256.HashData(bytes))));
        }
    }

    [Fact]
    public void Every_Migration_File_Is_Pinned()
    {
        var pinned = ReadManifest();

        var present = Directory
            .EnumerateFiles(ResolveMigrationsDirectory(), "*.cs")
            .Select(Path.GetFileName)
            .Where(name => name != "KodosiDbContextModelSnapshot.cs")
            .Order(StringComparer.Ordinal);

        Assert.Equal(pinned.Keys.Order(StringComparer.Ordinal), present);
    }

    private static IReadOnlyDictionary<string, (int Length, string Sha256)> ReadManifest()
        => File
            .ReadAllLines(
                Path.Combine(
                    AppContext.BaseDirectory,
                    "Infrastructure",
                    "historical-migration-bytes.tsv"))
            .Where(line => line.Length > 0)
            .Select(line => line.Split('\t'))
            .ToDictionary(
                fields => fields[0],
                fields => (int.Parse(fields[1], CultureInfo.InvariantCulture), fields[2]),
                StringComparer.Ordinal);

    private static string ResolveMigrationsDirectory()
        => Path.GetFullPath(
            Path.Combine(
                AppContext.BaseDirectory,
                "..",
                "..",
                "..",
                "..",
                "..",
                "src",
                "Kodosi.Infrastructure",
                "Persistence",
                "Migrations"));
}
