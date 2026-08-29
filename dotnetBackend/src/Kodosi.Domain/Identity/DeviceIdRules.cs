namespace Kodosi.Domain;

public static class DeviceIdRules
{
    public const int MaximumLength = IdentityWireFormat.DeviceIdMaxUtf16CodeUnits;

    public static string Require(string? value, string fieldName = "Device ID")
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            throw new DomainException($"{fieldName} is required.");
        }

        var normalized = value.Trim();
        if (normalized.Length > MaximumLength)
        {
            throw new DomainException(
                $"{fieldName} must be {MaximumLength} characters or fewer.");
        }
        return normalized;
    }

    public static string Normalize(string? value) => value?.Trim() ?? string.Empty;
}
