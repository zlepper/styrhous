using System.Data;
using System.Security.Claims;
using Microsoft.EntityFrameworkCore;
using OpenIddict.Abstractions;
using OpenIddict.Validation.AspNetCore;
using Styrhous.Licensing.Api.Devices;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Api.Desktop;

public static class DesktopEntitlementEndpoints
{
    private const int MaximumOptimisticAttempts = 3;
    private const string ConcurrentModificationReasonCode = "concurrent_modification";

    public static IEndpointRouteBuilder MapDesktopEntitlementEndpoints(
        this IEndpointRouteBuilder endpoints)
    {
        endpoints.MapGet(
            DesktopProtocolConstants.KeysPath,
            (DesktopLeaseSigner signer) => TypedResults.Ok(signer.GetJsonWebKeySet()));
        endpoints.MapPost(DesktopProtocolConstants.EntitlementPath, IssueAsync)
            .RequireDesktopAccessToken();
        endpoints.MapGet(DesktopProtocolConstants.DevicesPath, ListAsync)
            .RequireDesktopAccessToken();
        endpoints.MapDelete(
                $"{DesktopProtocolConstants.DevicesPath}/{{activationId}}",
                RevokeAsync)
            .RequireDesktopAccessToken();
        return endpoints;
    }

    private static async Task<IResult> IssueAsync(
        ClaimsPrincipal principal,
        IServiceScopeFactory scopeFactory,
        DesktopLeaseSigner signer,
        TimeProvider timeProvider,
        CancellationToken cancellationToken)
    {
        if (!TryContext(principal, out var userId, out var activationId))
        {
            return Error(StatusCodes.Status401Unauthorized, "invalid_access_token");
        }

        var observedAt = timeProvider.GetUtcNow().ToUniversalTime();
        for (var attempt = 0; attempt < MaximumOptimisticAttempts; attempt++)
        {
            try
            {
                await using var scope = scopeFactory.CreateAsyncScope();
                var dbContext = scope.ServiceProvider.GetRequiredService<LicensingDbContext>();
                var entitlementService = scope.ServiceProvider.GetRequiredService<DeviceEntitlementCheckService>();
                return await TryIssueAsync(
                    principal,
                    userId,
                    activationId,
                    entitlementService,
                    signer,
                    dbContext,
                    observedAt,
                    cancellationToken);
            }
            catch (Exception exception) when (EfConcurrencyFailure.IsRetryable(exception))
            {
                if (attempt == MaximumOptimisticAttempts - 1)
                {
                    return Error(
                        StatusCodes.Status409Conflict,
                        ConcurrentModificationReasonCode);
                }
            }
        }

        throw new InvalidOperationException("The offline lease retry loop did not complete.");
    }

    private static async Task<IResult> TryIssueAsync(
        ClaimsPrincipal principal,
        Guid userId,
        Guid activationId,
        DeviceEntitlementCheckService entitlementService,
        DesktopLeaseSigner signer,
        LicensingDbContext dbContext,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        return await LicensingDbContextTransaction.ExecuteAsync<IResult>(
            dbContext,
            IsolationLevel.Serializable,
            async (transaction, token) =>
            {
                var check = await entitlementService.CheckAtAsync(
                    userId,
                    activationId,
                    observedAt,
                    token);
                if (check.ReasonCode == EntitlementReasonCodes.SubscriptionRenewalPending)
                {
                    return Error(StatusCodes.Status503ServiceUnavailable, check.ReasonCode);
                }

                var lease = check.Status is DeviceEntitlementCheckStatus.Eligible
                    ? signer.TryIssue(userId, check, observedAt)
                    : null;
                if (lease is null)
                {
                    await RevokeAuthorizationAsync(
                        principal,
                        dbContext,
                        observedAt,
                        token);
                    await transaction.CommitAsync(token);
                    return Error(
                        StatusCodes.Status403Forbidden,
                        check.Status is DeviceEntitlementCheckStatus.Eligible
                            ? EntitlementReasonCodes.NoValidEntitlement
                            : check.ReasonCode);
                }

                await transaction.CommitAsync(token);
                return TypedResults.Ok(new
                {
                    reasonCode = "offline_lease_issued",
                    lease = lease.Lease,
                    state = lease.State,
                    entitlementReasonCode = lease.ReasonCode,
                    expiresAt = lease.ExpiresAt,
                    refreshAfter = lease.RefreshAfter,
                });
            },
            cancellationToken);
    }

    private static async Task<IResult> ListAsync(
        ClaimsPrincipal principal,
        DeviceListingService service,
        CancellationToken cancellationToken)
    {
        if (!TryUserAndSeat(principal, out var userId, out var seatId))
        {
            return Error(StatusCodes.Status401Unauthorized, "invalid_access_token");
        }

        var result = await service.ListActiveAsync(userId, seatId, cancellationToken);
        return result.Status is DeviceListingStatus.Listed
            ? TypedResults.Ok(DeviceListResponse.From(result))
            : Error(StatusCodes.Status404NotFound, result.ReasonCode);
    }

    private static async Task<IResult> RevokeAsync(
        string activationId,
        ClaimsPrincipal principal,
        DeviceRevocationService service,
        CancellationToken cancellationToken)
    {
        if (!Guid.TryParse(principal.FindFirstValue(
                OpenIddictConstants.Claims.Subject), out var userId))
        {
            return Error(StatusCodes.Status401Unauthorized, "invalid_access_token");
        }

        if (!Guid.TryParse(activationId, out var parsedActivationId))
        {
            return Error(StatusCodes.Status404NotFound, "device_not_active");
        }

        var result = await service.RevokeAsync(
            userId,
            parsedActivationId,
            cancellationToken);
        return result.Status is DeviceRevocationStatus.Revoked
            ? TypedResults.Ok(DeviceRevocationResponse.From(result))
            : Error(StatusCodes.Status404NotFound, result.ReasonCode);
    }

    private static bool TryContext(
        ClaimsPrincipal principal,
        out Guid userId,
        out Guid activationId)
    {
        activationId = Guid.Empty;
        return Guid.TryParse(
            principal.FindFirstValue(OpenIddictConstants.Claims.Subject),
            out userId)
        && Guid.TryParse(
            principal.FindFirstValue(DesktopProtocolConstants.Claims.ActivationId),
            out activationId);
    }

    private static bool TryUserAndSeat(
        ClaimsPrincipal principal,
        out Guid userId,
        out Guid seatId)
    {
        seatId = Guid.Empty;
        return Guid.TryParse(
            principal.FindFirstValue(OpenIddictConstants.Claims.Subject),
            out userId)
        && Guid.TryParse(
            principal.FindFirstValue(DesktopProtocolConstants.Claims.SeatId),
            out seatId);
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

    private static Microsoft.AspNetCore.Http.HttpResults.JsonHttpResult<
        DesktopProtocolErrorResponse> Error(int statusCode, string reasonCode)
    {
        return TypedResults.Json(
            new DesktopProtocolErrorResponse(reasonCode),
            statusCode: statusCode);
    }

    private static RouteHandlerBuilder RequireDesktopAccessToken(
        this RouteHandlerBuilder builder)
    {
        return builder
            .WithMetadata(DesktopAccessTokenMetadata.Instance)
            .RequireAuthorization(policy => policy
                .AddAuthenticationSchemes(
                    OpenIddictValidationAspNetCoreDefaults.AuthenticationScheme)
                .RequireAuthenticatedUser()
                .RequireAssertion(context =>
                    context.User.HasScope(DesktopProtocolConstants.Scope)));
    }
}

public sealed record DesktopProtocolErrorResponse(string ReasonCode);
