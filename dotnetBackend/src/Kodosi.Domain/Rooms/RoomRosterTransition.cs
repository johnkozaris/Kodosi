namespace Kodosi.Domain;

public sealed class RoomRosterTransition
{
    public RoomId RoomId { get; private set; }
    public long Generation { get; private set; }
    public byte[] RosterBody { get; private set; } = [];
    public byte[] RosterSignature { get; private set; } = [];
    public string RosterSignerDeviceId { get; private set; } = string.Empty;
    public Guid? AdmissionInvitationId { get; private set; }
    public DateTimeOffset CreatedAt { get; private set; }

    private RoomRosterTransition() { }

    public static RoomRosterTransition Create(Room room, Guid? admissionInvitationId = null)
    {
        if (room.RosterGeneration < 1 || room.RosterBody.Length == 0
            || room.RosterSignature.Length == 0)
        {
            throw new DomainException("A signed roster transition is required.");
        }

        return new RoomRosterTransition
        {
            RoomId = room.Id,
            Generation = room.RosterGeneration,
            RosterBody = room.RosterBody.ToArray(),
            RosterSignature = room.RosterSignature.ToArray(),
            RosterSignerDeviceId = room.RosterSignerDeviceId,
            AdmissionInvitationId = admissionInvitationId,
            CreatedAt = DateTimeOffset.UtcNow,
        };
    }
}
