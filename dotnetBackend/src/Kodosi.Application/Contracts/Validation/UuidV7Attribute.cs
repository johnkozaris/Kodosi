using System.ComponentModel.DataAnnotations;

namespace Kodosi.Application;

[AttributeUsage(AttributeTargets.Property | AttributeTargets.Field | AttributeTargets.Parameter)]
public sealed class UuidV7Attribute : ValidationAttribute
{
    public override bool IsValid(object? value)
    {
        return value switch
        {
            null => true,
            Guid guid => guid != Guid.Empty && guid.Version == 7,
            _ => false,
        };
    }
}
