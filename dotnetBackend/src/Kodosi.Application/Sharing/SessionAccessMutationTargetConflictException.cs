using Kodosi.Domain;

namespace Kodosi.Application;

public sealed class SessionAccessMutationTargetConflictException : ConflictException
{
    public SessionAccessMutationTargetConflictException()
        : base(
            "The session access mutation identifier was already used for a different target.",
            "SESSION_ACCESS_MUTATION_TARGET_CONFLICT")
    { }
}
