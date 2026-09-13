using System.Security.Claims;
using Microsoft.AspNetCore.Authentication;
using Microsoft.IdentityModel.Tokens;
using OpenIddict.Abstractions;
using OpenIddict.Server.AspNetCore;
using Styrhous.Licensing.Api.Antiforgery;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Application.Desktop;
using Styrhous.Licensing.Domain.Devices;
using static OpenIddict.Abstractions.OpenIddictConstants;

namespace Styrhous.Licensing.Api.Desktop;

public static class DesktopDeviceAuthorizationEndpoints
{
    private const int MaximumUserCodeLength = 64;

    public static IEndpointRouteBuilder MapDesktopDeviceAuthorizationEndpoints(
        this IEndpointRouteBuilder endpoints)
    {
        ArgumentNullException.ThrowIfNull(endpoints);
        endpoints.MapGet(DesktopProtocolConstants.ApprovalPath, GetApprovalAsync)
            .RequireAuthorization();
        endpoints.MapPost(DesktopProtocolConstants.ApprovalPath, DecideAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        return endpoints;
    }

    private static async Task<IResult> GetApprovalAsync(
        HttpContext httpContext,
        string? user_code,
        DesktopDeviceAuthorizationService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(httpContext.User, out var userId)
            || !IsValidUserCode(user_code))
        {
            return TypedResults.NotFound(
                DesktopDeviceAuthorizationErrorResponse.NotFound());
        }

        var authorization = await httpContext.AuthenticateAsync(
            OpenIddictServerAspNetCoreDefaults.AuthenticationScheme);
        var authorizationPrincipal = authorization.Succeeded
            ? authorization.Principal
            : null;
        var installation = authorizationPrincipal is not null
            ? DesktopInstallationProtocolClaims.Read(authorizationPrincipal)
            : null;
        if (installation is null || authorizationPrincipal is null)
        {
            return TypedResults.NotFound(
                DesktopDeviceAuthorizationErrorResponse.NotFound());
        }

        var approval = await service.GetApprovalAsync(
            userId,
            installation,
            cancellationToken);
        return TypedResults.Ok(
            DesktopDeviceAuthorizationApprovalResponse.From(approval));
    }

    private static async Task<IResult> DecideAsync(
        HttpContext httpContext,
        DesktopProtocolTransaction transaction,
        DesktopDeviceAuthorizationService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(httpContext.User, out var userId))
        {
            return TypedResults.Unauthorized();
        }

        var form = await httpContext.Request.ReadFormAsync(cancellationToken);
        if (HasDuplicateValue(form, Parameters.UserCode)
            || HasDuplicateValue(form, "decision")
            || HasDuplicateValue(form, "seat_id"))
        {
            return TypedResults.BadRequest(
                DesktopDeviceAuthorizationErrorResponse.InvalidRequest());
        }

        var userCode = form[Parameters.UserCode].SingleOrDefault();
        if (!IsValidUserCode(userCode))
        {
            return TypedResults.NotFound(
                DesktopDeviceAuthorizationErrorResponse.NotFound());
        }

        var authorization = await httpContext.AuthenticateAsync(
            OpenIddictServerAspNetCoreDefaults.AuthenticationScheme);
        var authorizationPrincipal = authorization.Succeeded
            ? authorization.Principal
            : null;
        var installation = authorizationPrincipal is not null
            ? DesktopInstallationProtocolClaims.Read(authorizationPrincipal)
            : null;
        if (installation is null || authorizationPrincipal is null)
        {
            return TypedResults.NotFound(
                DesktopDeviceAuthorizationErrorResponse.NotFound());
        }

        if (!Guid.TryParse(
                authorizationPrincipal.GetAuthorizationId(),
                out var authorizationId)
            || !Guid.TryParse(
                authorizationPrincipal.GetTokenId(),
                out var userCodeTokenId))
        {
            return TypedResults.NotFound(
                DesktopDeviceAuthorizationErrorResponse.NotFound());
        }

        var decision = form["decision"].SingleOrDefault();
        if (string.Equals(decision, "deny", StringComparison.Ordinal))
        {
            await transaction.EnsureStartedAsync(cancellationToken);
            DesktopProtocolRequestState.SetAuthorizationDecision(
                httpContext,
                userCodeTokenId,
                DesktopAuthorizationDecision.Deny);
            return Results.Forbid(
                new AuthenticationProperties(),
                [OpenIddictServerAspNetCoreDefaults.AuthenticationScheme]);
        }

        if (!string.Equals(decision, "approve", StringComparison.Ordinal)
            || !Guid.TryParse(form["seat_id"].SingleOrDefault(), out var seatId))
        {
            return TypedResults.Conflict(
                DesktopDeviceAuthorizationErrorResponse.SeatRequired());
        }

        await transaction.EnsureStartedAsync(cancellationToken);
        var result = await service.ApproveAsync(
            userId,
            seatId,
            authorizationId,
            installation,
            cancellationToken);
        return result.Status switch
        {
            DesktopDeviceAuthorizationDecisionStatus.Approved =>
                Approve(
                    httpContext,
                    userCodeTokenId,
                    authorizationPrincipal!,
                    userId,
                    seatId,
                    installation,
                    result),
            DesktopDeviceAuthorizationDecisionStatus.SeatNotEligible =>
                TypedResults.Conflict(
                    DesktopDeviceAuthorizationErrorResponse.SeatNotEligible()),
            DesktopDeviceAuthorizationDecisionStatus.DeviceLimitReached =>
                TypedResults.Conflict(
                    DesktopDeviceAuthorizationCapacityResponse.From(result)),
            _ => throw new InvalidOperationException(
                $"Unsupported desktop authorization status: {result.Status}."),
        };
    }

    private static IResult Approve(
        HttpContext httpContext,
        Guid userCodeTokenId,
        ClaimsPrincipal authorization,
        Guid userId,
        Guid seatId,
        DesktopInstallation installation,
        DesktopDeviceAuthorizationDecisionResult result)
    {
        var identity = new ClaimsIdentity(
            TokenValidationParameters.DefaultAuthenticationType,
            Claims.Name,
            Claims.Role);
        var principal = new ClaimsPrincipal(identity)
            .SetClaim(Claims.Subject, userId.ToString())
            .SetClaim(DesktopProtocolConstants.Claims.SeatId, seatId.ToString())
            .SetClaim(
                DesktopProtocolConstants.Claims.ActivationId,
                result.ActivationId!.Value.ToString())
            .SetScopes(authorization.GetScopes())
            .SetResources(DesktopProtocolConstants.Resource)
            .SetInstallation(installation);
        DesktopProtocolRequestState.SetAuthorizationDecision(
            httpContext,
            userCodeTokenId,
            DesktopAuthorizationDecision.Approve);
        return Results.SignIn(
            principal,
            new AuthenticationProperties(),
            OpenIddictServerAspNetCoreDefaults.AuthenticationScheme);
    }

    private static bool IsValidUserCode(string? userCode)
    {
        return !string.IsNullOrWhiteSpace(userCode)
        && userCode.Length <= MaximumUserCodeLength;
    }

    private static bool HasDuplicateValue(IFormCollection form, string key)
    {
        return form[key].Count > 1;
    }
}
