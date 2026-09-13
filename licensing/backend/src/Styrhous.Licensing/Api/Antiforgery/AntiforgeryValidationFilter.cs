using Microsoft.AspNetCore.Antiforgery;

namespace Styrhous.Licensing.Api.Antiforgery;

internal sealed class AntiforgeryValidationFilter(IAntiforgery antiforgery) : IEndpointFilter
{

    public async ValueTask<object?> InvokeAsync(
        EndpointFilterInvocationContext context,
        EndpointFilterDelegate next)
    {
        try
        {
            await antiforgery.ValidateRequestAsync(context.HttpContext);
        }
        catch (AntiforgeryValidationException)
        {
            return TypedResults.BadRequest();
        }

        return await next(context);
    }
}
