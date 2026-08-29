namespace Kodosi.Infrastructure.Persistence.Configurations;

internal static class EnumCheckConstraint
{
    public static string AllowedValues<TEnum>(string columnName)
        where TEnum : struct, Enum
    {
        var values = string.Join(
            ", ",
            Enum.GetNames<TEnum>().Select(static name => $"'{name}'"));
        return $"\"{columnName}\" IN ({values})";
    }
}
