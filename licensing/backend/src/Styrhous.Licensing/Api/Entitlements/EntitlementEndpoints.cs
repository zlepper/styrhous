using System.Security.Claims;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Entitlements;

namespace Styrhous.Licensing.Api.Entitlements;

public static class EntitlementEndpoints
{
    public static IEndpointRouteBuilder MapEntitlementEndpoints(
        this IEndpointRouteBuilder endpoints)
    {
        endpoints.MapGet("/api/entitlements", ListAsync)
            .RequireAuthorization();
        return endpoints;
    }

    private static async Task<IResult> ListAsync(
        ClaimsPrincipal principal,
        EntitlementResolutionService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var userId))
        {
            return TypedResults.Unauthorized();
        }

        try
        {
            var entitlements = await service.ListForUserAsync(userId, cancellationToken);
            return TypedResults.Ok(EntitlementListResponse.From(entitlements));
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }
}
