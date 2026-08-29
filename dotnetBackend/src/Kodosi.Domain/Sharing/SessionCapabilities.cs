namespace Kodosi.Domain;

public enum SessionCapability
{
    View = 0,
    Suggest = 1,
    SendInput = 2,
    ApproveDeny = 3,
    Resize = 4,
    Focus = 5,
    Stop = 6,
}

public readonly record struct SessionCapabilities(
    bool View,
    bool Suggest,
    bool SendInput,
    bool ApproveDeny,
    bool Resize,
    bool Focus,
    bool Stop)
{
    public static SessionCapabilities FromAccess(
        AccessLevel access,
        bool isOwnerParticipant = false)
    {
        if (isOwnerParticipant)
        {
            return new(
                View: true,
                Suggest: true,
                SendInput: true,
                ApproveDeny: true,
                Resize: true,
                Focus: true,
                Stop: true);
        }

        return access switch
        {
            AccessLevel.View => new(
                true, false, false, false, false, false, false),
            AccessLevel.Suggest => new(
                true, true, false, false, false, false, false),
            AccessLevel.Inject => new(
                true, true, true, false, false, true, false),
            AccessLevel.Approve => new(
                true, false, false, true, false, false, false),
            _ => default,
        };
    }

    public bool Allows(SessionCapability capability) => capability switch
    {
        SessionCapability.View => View,
        SessionCapability.Suggest => Suggest,
        SessionCapability.SendInput => SendInput,
        SessionCapability.ApproveDeny => ApproveDeny,
        SessionCapability.Resize => Resize,
        SessionCapability.Focus => Focus,
        SessionCapability.Stop => Stop,
        _ => false,
    };

    public bool CanReceivePendingPermissions =>
        Suggest || SendInput || ApproveDeny;

    public ushort ToMask()
    {
        ushort mask = 0;
        if (View) mask |= 1 << (int)SessionCapability.View;
        if (Suggest) mask |= 1 << (int)SessionCapability.Suggest;
        if (SendInput) mask |= 1 << (int)SessionCapability.SendInput;
        if (ApproveDeny) mask |= 1 << (int)SessionCapability.ApproveDeny;
        if (Resize) mask |= 1 << (int)SessionCapability.Resize;
        if (Focus) mask |= 1 << (int)SessionCapability.Focus;
        if (Stop) mask |= 1 << (int)SessionCapability.Stop;
        return mask;
    }
}
