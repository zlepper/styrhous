using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Authorization.Policy;

namespace Styrhous.Licensing.Api.Desktop;

public sealed class DesktopAuthorizationResultHandler : IAuthorizationMiddlewareResultHandler
{
    private readonly AuthorizationMiddlewareResultHandler _fallback = new();

    public Task HandleAsync(
        RequestDelegate next,
        HttpContext context,
        AuthorizationPolicy policy,
        PolicyAuthorizationResult authorizeResult)
    {
        if (context.GetEndpoint()?.Metadata.GetMetadata<DesktopAccessTokenMetadata>() is null
            || authorizeResult.Succeeded)
        {
            return _fallback.HandleAsync(
                next,
                context,
                policy,
                authorizeResult);
        }

        var forbidden = authorizeResult.Forbidden;
        context.Response.StatusCode = forbidden
            ? StatusCodes.Status403Forbidden
            : StatusCodes.Status401Unauthorized;
        context.Response.ContentType = "application/json";
        if (!forbidden)
        {
            context.Response.Headers.WWWAuthenticate = "Bearer";
        }

        return context.Response.WriteAsJsonAsync(new DesktopProtocolErrorResponse(
            forbidden ? "insufficient_scope" : "invalid_access_token"));
    }
}

internal sealed class DesktopAccessTokenMetadata
{
    public static DesktopAccessTokenMetadata Instance { get; } = new();

    private DesktopAccessTokenMetadata()
    {
    }
}
