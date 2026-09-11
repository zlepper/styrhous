using Microsoft.AspNetCore.Antiforgery;

namespace Styrhous.Licensing.Api.Antiforgery;

public static class AntiforgeryEndpoints
{
    public const string HeaderName = "X-CSRF-TOKEN";

    public static IEndpointRouteBuilder MapAntiforgeryEndpoints(
        this IEndpointRouteBuilder endpoints)
    {
        endpoints.MapGet(
                "/api/antiforgery",
                (HttpContext context, IAntiforgery antiforgery) =>
                {
                    var tokens = antiforgery.GetAndStoreTokens(context);
                    return TypedResults.Ok(
                        new AntiforgeryTokenResponse(
                            tokens.RequestToken
                                ?? throw new InvalidOperationException(
                                    "An antiforgery request token was not generated.")));
                })
            .RequireAuthorization();
        return endpoints;
    }

    private sealed record AntiforgeryTokenResponse(string RequestToken);
}
