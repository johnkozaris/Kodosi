namespace Kodosi.Domain;

public readonly record struct RoomId(Guid Value)
{
    public static RoomId From(Guid value) => new(value);
    public override string ToString() => Value.ToString();
}
