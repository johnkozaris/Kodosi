namespace Kodosi.Domain;

public static class RoomInputRules
{
    public const int RoomNameMaxLength = 128;
    public const int RoomSlugMaxLength = 64;
    public const string RoomSlugPattern = "^[a-z0-9-]{3,64}$";
    public const int ChatRecipientMaxCount = 32;
    public const int TaskTitleMaxLength = 200;
    public const int TaskDescriptionMaxLength = 4000;
    public const int TaskResultMaxLength = 4000;
    public const int EncryptedContentMaxLength = 1024 * 1024;
}
