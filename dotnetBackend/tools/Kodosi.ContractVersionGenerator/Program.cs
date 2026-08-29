using System.Text.Json;

if (args.Length != 2)
{
    Console.Error.WriteLine("usage: ContractVersionGenerator <authority-json> <output-cs>");
    return 2;
}

using var document = JsonDocument.Parse(File.ReadAllText(args[0]));
var root = document.RootElement;
var apiContractVersion = ReadNonNegativeInt32(root, "apiContractVersion");
var authContractVersion = ReadNonNegativeInt32(root, "authContractVersion");
var source = $$"""
namespace Kodosi.Host.DependencyInjection;

internal static class BackendContractVersions
{
    internal const int Api = {{apiContractVersion}};
    internal const int Auth = {{authContractVersion}};
}
""";

var outputPath = Path.GetFullPath(args[1]);
Directory.CreateDirectory(Path.GetDirectoryName(outputPath)!);
if (!File.Exists(outputPath)
    || !string.Equals(File.ReadAllText(outputPath), source, StringComparison.Ordinal))
{
    File.WriteAllText(outputPath, source);
}
return 0;

static int ReadNonNegativeInt32(JsonElement root, string propertyName)
{
    if (!root.TryGetProperty(propertyName, out var property)
        || !property.TryGetInt32(out var value)
        || value < 0)
    {
        throw new InvalidDataException(
            $"Backend API authority requires a non-negative Int32 '{propertyName}'.");
    }

    return value;
}
