
namespace Kodosi.Domain;

public static class RuntimeAccessGate
{
    public static bool Allows(
        SessionCapabilities capabilities,
        SessionCapability requiredCapability,
        bool ownerPresent,
        SessionStatus sessionStatus)
    {
        if (sessionStatus == SessionStatus.Ended)
        {
            return false;
        }

        if (requiredCapability == SessionCapability.View)
        {
            return capabilities.View;
        }

        return sessionStatus == SessionStatus.Live
            && ownerPresent
            && capabilities.Allows(requiredCapability);
    }
}
