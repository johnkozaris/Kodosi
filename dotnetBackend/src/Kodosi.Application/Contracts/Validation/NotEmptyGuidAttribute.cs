using System.ComponentModel.DataAnnotations;

namespace Kodosi.Application;

[AttributeUsage(AttributeTargets.Property | AttributeTargets.Field | AttributeTargets.Parameter)]
public sealed class NotEmptyGuidAttribute : ValidationAttribute
{
    public override bool IsValid(object? value)
    {
        return value switch
        {
            null => true,
            Guid guid => guid != Guid.Empty,
            _ => false,
        };
    }
}
