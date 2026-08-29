namespace Kodosi.Application;

public enum SessionTransitionOutcome
{
    Applied,
    AlreadyInTargetState,
    NotFound,
    Rejected,
}

public static class SessionTransitionOutcomeExtensions
{
    public static bool SatisfiesTargetState(this SessionTransitionOutcome outcome)
    {
        return outcome is SessionTransitionOutcome.Applied or SessionTransitionOutcome.AlreadyInTargetState;
    }
}
