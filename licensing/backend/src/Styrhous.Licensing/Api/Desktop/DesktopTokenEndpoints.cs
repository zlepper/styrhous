using System.Security.Claims;
using Microsoft.AspNetCore;
using Microsoft.AspNetCore.Authentication;
using Microsoft.IdentityModel.Tokens;
using OpenIddict.Abstractions;
using OpenIddict.Server.AspNetCore;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Persistence;
using static OpenIddict.Abstractions.OpenIddictConstants;

namespace Styrhous.Licensing.Api.Desktop;

public static class DesktopTokenEndpoints
{
    public static IEndpointRouteBuilder MapDesktopTokenEndpoints(
        this IEndpointRouteBuilder endpoints)
    {
        ArgumentNullException.ThrowIfNull(endpoints);
        endpoints.MapPost(DesktopProtocolConstants.TokenPath, ExchangeAsync);
        return endpoints;
    }

    private static async Task<IResult> ExchangeAsync(
        HttpContext httpContext,
        DesktopProtocolTransaction transaction,
        DeviceEntitlementCheckService entitlementService,
        LicensingDbContext dbContext,
        TimeProvider timeProvider,
        CancellationToken cancellationToken)
    {
        var request = httpContext.GetOpenIddictServerRequest()
            ?? throw new InvalidOperationException(
                "The OpenIddict token request is unavailable.");
        if (!request.IsDeviceCodeGrantType() && !request.IsRefreshTokenGrantType())
        {
            return OAuthError(
                Errors.UnsupportedGrantType,
                "The requested desktop token grant is not supported.");
        }

        var authentication = await httpContext.AuthenticateAsync(
            OpenIddictServerAspNetCoreDefaults.AuthenticationScheme);
        var principal = authentication.Principal;
        if (!authentication.Succeeded
            || principal is null
            || !Guid.TryParse(principal.GetClaim(Claims.Subject), out var userId)
            || !Guid.TryParse(principal.GetAuthorizationId(), out var authorizationId)
            || !Guid.TryParse(principal.GetTokenId(), out var tokenId)
            || !Guid.TryParse(
                principal.GetClaim(DesktopProtocolConstants.Claims.ActivationId),
                out var activationId)
            || (request.IsRefreshTokenGrantType()
                && principal.GetExpirationDate() is { } refreshExpiresAt
                && timeProvider.GetUtcNow() >= refreshExpiresAt))
        {
            return OAuthError(Errors.InvalidGrant, "The desktop authorization is invalid.");
        }

        DesktopProtocolRequestState.SetTokenGrant(
            httpContext,
            userId,
            authorizationId,
            tokenId,
            request.IsRefreshTokenGrantType()
                ? DesktopTokenGrant.RefreshToken
                : DesktopTokenGrant.DeviceCode);
        await transaction.EnsureStartedAsync(cancellationToken);
        var check = await entitlementService.CheckAsync(
            userId,
            activationId,
            cancellationToken);
        if (check.ReasonCode == EntitlementReasonCodes.SubscriptionRenewalPending)
        {
            transaction.MarkRetryableFailure();
            return Results.Json(new
            {
                error = Errors.TemporarilyUnavailable,
                error_description = "The subscription renewal has not been confirmed yet. Retry with the same credential.",
            }, statusCode: StatusCodes.Status503ServiceUnavailable);
        }
        if (check.Status is not DeviceEntitlementCheckStatus.Eligible
            || check.Entitlement is null
            || check.Device is null)
        {
            await RevokeAuthorizationAsync(
                principal,
                dbContext,
                timeProvider.GetUtcNow(),
                cancellationToken);
            return OAuthError(
                Errors.InvalidGrant,
                "The desktop device no longer has an eligible seat.");
        }

        var installation = DesktopInstallation.Create(
            check.Device.InstallationId,
            check.Device.DisplayName,
            check.Device.Platform,
            check.Device.Architecture,
            check.Device.StyrhousVersion);
        var identity = new ClaimsIdentity(
            principal.Claims,
            TokenValidationParameters.DefaultAuthenticationType,
            Claims.Name,
            Claims.Role);
        var refreshed = new ClaimsPrincipal(identity)
            .SetClaim(Claims.Subject, userId.ToString())
            .SetClaim(
                DesktopProtocolConstants.Claims.SeatId,
                check.Entitlement.SeatId.ToString())
            .SetClaim(
                DesktopProtocolConstants.Claims.BillingAccountId,
                check.Entitlement.BillingAccountId.ToString())
            .SetClaim(
                DesktopProtocolConstants.Claims.ActivationId,
                activationId.ToString())
            .SetClaim(
                DesktopProtocolConstants.Claims.EntitlementState,
                check.Entitlement.State.ToString().ToLowerInvariant())
            .SetClaim(
                DesktopProtocolConstants.Claims.EntitlementReasonCode,
                check.Entitlement.ReasonCode)
            .SetScopes(principal.GetScopes())
            .SetResources(DesktopProtocolConstants.Resource)
            .SetInstallation(installation);
        identity.SetDestinations(GetDestinations);
        return Results.SignIn(
            refreshed,
            new AuthenticationProperties(),
            OpenIddictServerAspNetCoreDefaults.AuthenticationScheme);
    }

    private static IResult OAuthError(string error, string description)
    {
        return Results.Forbid(
            new AuthenticationProperties(
                new Dictionary<string, string?>(StringComparer.Ordinal)
                {
                    [OpenIddictServerAspNetCoreConstants.Properties.Error] = error,
                    [OpenIddictServerAspNetCoreConstants.Properties.ErrorDescription] =
                        description,
                }),
            [OpenIddictServerAspNetCoreDefaults.AuthenticationScheme]);
    }

    private static async Task RevokeAuthorizationAsync(
        ClaimsPrincipal principal,
        LicensingDbContext dbContext,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        if (Guid.TryParse(principal.GetAuthorizationId(), out var authorizationId))
        {
            await DesktopSessionRevocation.RevokeDesktopAuthorizationsAsync(
                dbContext,
                [authorizationId],
                observedAt,
                cancellationToken);
            await dbContext.SaveChangesAsync(cancellationToken);
        }
    }

    private static IEnumerable<string> GetDestinations(Claim claim)
    {
        return claim.Type switch
        {
            Claims.Subject
                or DesktopProtocolConstants.Claims.InstallationId
                or DesktopProtocolConstants.Claims.SeatId
                or DesktopProtocolConstants.Claims.BillingAccountId
                or DesktopProtocolConstants.Claims.ActivationId
                or DesktopProtocolConstants.Claims.EntitlementState
                or DesktopProtocolConstants.Claims.EntitlementReasonCode =>
                [Destinations.AccessToken],
            _ => [],
        };
    }
}
