using System.ComponentModel.DataAnnotations;

namespace Kodosi.Host.Endpoints;

public sealed class DataAnnotationsValidationFilter<TRequest> : IEndpointFilter
    where TRequest : class
{
    public async ValueTask<object?> InvokeAsync(
        EndpointFilterInvocationContext context,
        EndpointFilterDelegate next)
    {
        var request = context.Arguments.OfType<TRequest>().FirstOrDefault();
        if (request is null)
        {
            return await next(context);
        }

        var validationResults = new List<ValidationResult>();
        var validationContext = new ValidationContext(request);

        if (Validator.TryValidateObject(request, validationContext, validationResults, validateAllProperties: true))
        {
            return await next(context);
        }

        var errors = validationResults
            .SelectMany(result =>
            {
                var memberNames = result.MemberNames.Any() ? result.MemberNames : [string.Empty];
                return memberNames.Select(memberName => new
                {
                    MemberName = memberName,
                    ErrorMessage = result.ErrorMessage ?? "The request is invalid.",
                });
            })
            .GroupBy(item => item.MemberName, item => item.ErrorMessage)
            .ToDictionary(group => group.Key, group => group.Distinct().ToArray());

        return Results.ValidationProblem(errors);
    }
}
