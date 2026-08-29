namespace Kodosi.Domain;

public enum DefaultAudienceAccess
{
    View = 0,
    Suggest = 1,
}

public static class DefaultAudienceAccessExtensions
{
    public static AccessLevel ToAccessLevel(this DefaultAudienceAccess access) =>
        access switch
        {
            DefaultAudienceAccess.View => AccessLevel.View,
            DefaultAudienceAccess.Suggest => AccessLevel.Suggest,
            _ => throw new DomainException("Default audience access is invalid."),
        };
}
