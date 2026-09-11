using System.Security.Claims;
using Microsoft.AspNetCore.Http.HttpResults;
using Styrhous.Licensing.Api.Antiforgery;
using Styrhous.Licensing.Api.Authentication;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Api.Billing;

public static class BillingAccountEndpoints
{
    public static IEndpointRouteBuilder MapBillingAccountEndpoints(
        this IEndpointRouteBuilder endpoints)
    {
        endpoints.MapGet("/api/billing-accounts", ListAsync)
            .RequireAuthorization();
        endpoints.MapGet("/api/billing-prices", ListPricesAsync)
            .RequireAuthorization();
        endpoints.MapPost(
                "/api/billing-accounts/{billingAccountId}/checkout-sessions",
                CreateCheckoutSessionAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapPost(
                "/api/billing-accounts/{billingAccountId}/customer-portal-sessions",
                CreateCustomerPortalSessionAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        endpoints.MapPatch(
                "/api/billing-accounts/{billingAccountId}/seat-quantity",
                ChangeSeatQuantityAsync)
            .RequireAuthorization()
            .AddEndpointFilter<AntiforgeryValidationFilter>();
        return endpoints;
    }

    private static async Task<IResult> ListPricesAsync(
        IBillingPriceProvider provider,
        CancellationToken cancellationToken)
    {
        try
        {
            return TypedResults.Ok(new { Prices = await provider.ListAsync(cancellationToken) });
        }
        catch (BillingPricesUnavailableException)
        {
            return BillingError(StatusCodes.Status503ServiceUnavailable, "billing_prices_unavailable");
        }
    }

    private static async Task<IResult> ChangeSeatQuantityAsync(
        string billingAccountId,
        ClaimsPrincipal principal,
        BillingSeatQuantityRequest request,
        BillingSeatQuantityService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(billingAccountId, out var parsedBillingAccountId))
        {
            return BillingError(
                StatusCodes.Status404NotFound,
                BillingSeatQuantityReasonCodes.BillingAccountNotFound);
        }

        Guid? retryOperationId = request.BillingOperationId is null
            ? null
            : Guid.Parse(request.BillingOperationId);

        try
        {
            var result = await service.ChangeAsync(
                actorUserId,
                parsedBillingAccountId,
                request!.SeatQuantity,
                retryOperationId,
                cancellationToken);
            return ToSeatQuantityResponse(result);
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> CreateCustomerPortalSessionAsync(
        string billingAccountId,
        ClaimsPrincipal principal,
        BillingCustomerPortalService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(billingAccountId, out var parsedBillingAccountId))
        {
            return BillingError(
                StatusCodes.Status404NotFound,
                BillingReasonCodes.BillingAccountNotFound);
        }

        try
        {
            var result = await service.CreateSessionAsync(
                actorUserId,
                parsedBillingAccountId,
                cancellationToken);
            return ToCustomerPortalResponse(result);
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> CreateCheckoutSessionAsync(
        string billingAccountId,
        ClaimsPrincipal principal,
        BillingCheckoutRequest request,
        BillingCheckoutService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var actorUserId))
        {
            return TypedResults.Unauthorized();
        }

        if (!Guid.TryParse(billingAccountId, out var parsedBillingAccountId))
        {
            return BillingError(
                StatusCodes.Status404NotFound,
                BillingCheckoutReasonCodes.BillingAccountNotFound);
        }

        var cadence = string.Equals(request.Cadence!.Trim(), "monthly", StringComparison.OrdinalIgnoreCase)
            ? BillingCadence.Monthly
            : BillingCadence.Annual;
        Guid? retryOperationId = request.BillingOperationId is null
            ? null
            : Guid.Parse(request.BillingOperationId);

        try
        {
            var result = await service.CreateSessionAsync(
                actorUserId,
                parsedBillingAccountId,
                cadence,
                request!.SeatQuantity,
                retryOperationId,
                cancellationToken);
            return ToCheckoutResponse(result);
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static async Task<IResult> ListAsync(
        ClaimsPrincipal principal,
        BillingAccountListingService service,
        CancellationToken cancellationToken)
    {
        if (!AuthenticatedUser.TryGetId(principal, out var userId))
        {
            return TypedResults.Unauthorized();
        }

        try
        {
            var accounts = await service.ListForUserAsync(userId, cancellationToken);
            return TypedResults.Ok(BillingAccountListResponse.From(accounts));
        }
        catch (UserNotFoundException)
        {
            return TypedResults.Unauthorized();
        }
    }

    private static IResult ToCheckoutResponse(BillingCheckoutResult result)
    {
        return result.Status switch
        {
            BillingCheckoutStatus.Created => TypedResults.Ok(
                new BillingCheckoutResponse(
                    BillingCheckoutReasonCodes.SessionCreated,
                    result.BillingOperationId
                        ?? throw MissingCheckoutResult(nameof(result.BillingOperationId)),
                    result.RedirectUri?.ToString()
                        ?? throw MissingCheckoutResult(nameof(result.RedirectUri)))),
            BillingCheckoutStatus.BillingAccountNotFound => BillingError(
                StatusCodes.Status404NotFound,
                BillingCheckoutReasonCodes.BillingAccountNotFound),
            BillingCheckoutStatus.BillingOperationNotFound => BillingError(
                StatusCodes.Status404NotFound,
                BillingCheckoutReasonCodes.BillingOperationNotFound),
            BillingCheckoutStatus.InsufficientPermission => BillingError(
                StatusCodes.Status403Forbidden,
                BillingCheckoutReasonCodes.InsufficientPermission),
            BillingCheckoutStatus.PersonalSeatQuantityInvalid => CapacityError(
                BillingCheckoutReasonCodes.PersonalSeatQuantityInvalid,
                result),
            BillingCheckoutStatus.SeatQuantityTooSmall => CapacityError(
                BillingCheckoutReasonCodes.SeatQuantityTooSmall,
                result),
            BillingCheckoutStatus.SubscriptionAlreadyExists => BillingError(
                StatusCodes.Status409Conflict,
                BillingCheckoutReasonCodes.SubscriptionAlreadyExists),
            BillingCheckoutStatus.CheckoutOperationInProgress => InProgressError(result),
            BillingCheckoutStatus.CheckoutOperationCapacityChanged =>
                CapacityChangedError(result),
            BillingCheckoutStatus.ProviderUnavailable => TypedResults.Json(
                new BillingCheckoutProviderErrorResponse(
                    BillingCheckoutReasonCodes.ProviderUnavailable,
                    result.BillingOperationId
                        ?? throw MissingCheckoutResult(nameof(result.BillingOperationId))),
                statusCode: StatusCodes.Status503ServiceUnavailable),
            _ => throw new ArgumentOutOfRangeException(nameof(result)),
        };
    }

    private static IResult ToCustomerPortalResponse(
        BillingCustomerPortalResult result)
    {
        return result.Status switch
        {
            BillingCustomerPortalStatus.Created => TypedResults.Ok(
                new BillingCustomerPortalResponse(
                    BillingCustomerPortalReasonCodes.SessionCreated,
                    result.RedirectUri?.ToString()
                        ?? throw MissingCustomerPortalResult(nameof(result.RedirectUri)))),
            BillingCustomerPortalStatus.BillingAccountNotFound => BillingError(
                StatusCodes.Status404NotFound,
                BillingReasonCodes.BillingAccountNotFound),
            BillingCustomerPortalStatus.InsufficientPermission => BillingError(
                StatusCodes.Status403Forbidden,
                BillingReasonCodes.InsufficientPermission),
            BillingCustomerPortalStatus.SubscriptionNotFound => BillingError(
                StatusCodes.Status409Conflict,
                BillingReasonCodes.SubscriptionNotFound),
            BillingCustomerPortalStatus.ProviderUnavailable => BillingError(
                StatusCodes.Status503ServiceUnavailable,
                BillingCustomerPortalReasonCodes.ProviderUnavailable),
            _ => throw new ArgumentOutOfRangeException(nameof(result)),
        };
    }

    private static IResult ToSeatQuantityResponse(BillingSeatQuantityResult result)
    {
        return result.Status switch
        {
            BillingSeatQuantityStatus.Changed => TypedResults.Ok(
                new BillingSeatQuantityResponse(
                    BillingSeatQuantityReasonCodes.Changed,
                    result.BillingOperationId
                        ?? throw MissingSeatQuantityResult(
                            nameof(result.BillingOperationId)),
                    result.AuthoritativeSeatQuantity
                        ?? throw MissingSeatQuantityResult(
                            nameof(result.AuthoritativeSeatQuantity)))),
            BillingSeatQuantityStatus.BillingAccountNotFound => BillingError(
                StatusCodes.Status404NotFound,
                BillingSeatQuantityReasonCodes.BillingAccountNotFound),
            BillingSeatQuantityStatus.BillingOperationNotFound => BillingError(
                StatusCodes.Status404NotFound,
                BillingSeatQuantityReasonCodes.BillingOperationNotFound),
            BillingSeatQuantityStatus.InsufficientPermission => BillingError(
                StatusCodes.Status403Forbidden,
                BillingSeatQuantityReasonCodes.InsufficientPermission),
            BillingSeatQuantityStatus.PersonalSeatQuantityInvalid => CapacityError(
                BillingSeatQuantityReasonCodes.PersonalSeatQuantityInvalid,
                result.RequiredSeatQuantity),
            BillingSeatQuantityStatus.SeatQuantityTooSmall => CapacityError(
                BillingSeatQuantityReasonCodes.SeatQuantityTooSmall,
                result.RequiredSeatQuantity),
            BillingSeatQuantityStatus.SeatQuantityUnchanged => BillingError(
                StatusCodes.Status409Conflict,
                BillingSeatQuantityReasonCodes.SeatQuantityUnchanged),
            BillingSeatQuantityStatus.SubscriptionNotFound => BillingError(
                StatusCodes.Status409Conflict,
                BillingSeatQuantityReasonCodes.SubscriptionNotFound),
            BillingSeatQuantityStatus.SubscriptionInactive => BillingError(
                StatusCodes.Status409Conflict,
                BillingSeatQuantityReasonCodes.SubscriptionInactive),
            BillingSeatQuantityStatus.OperationInProgress =>
                SeatQuantityOperationError(
                    StatusCodes.Status409Conflict,
                    BillingSeatQuantityReasonCodes.OperationInProgress,
                    result),
            BillingSeatQuantityStatus.SubscriptionQuantityChanged => TypedResults.Json(
                new BillingSeatQuantityStateErrorResponse(
                    BillingSeatQuantityReasonCodes.SubscriptionQuantityChanged,
                    result.AuthoritativeSeatQuantity
                        ?? throw MissingSeatQuantityResult(
                            nameof(result.AuthoritativeSeatQuantity))),
                statusCode: StatusCodes.Status409Conflict),
            BillingSeatQuantityStatus.ProviderUnavailable =>
                SeatQuantityOperationError(
                    StatusCodes.Status503ServiceUnavailable,
                    BillingSeatQuantityReasonCodes.ProviderUnavailable,
                    result),
            BillingSeatQuantityStatus.ProviderReconciliationRequired =>
                SeatQuantityOperationError(
                    StatusCodes.Status409Conflict,
                    BillingSeatQuantityReasonCodes.ProviderReconciliationRequired,
                    result),
            _ => throw new ArgumentOutOfRangeException(nameof(result)),
        };
    }

    private static JsonHttpResult<BillingCheckoutInProgressErrorResponse> InProgressError(
        BillingCheckoutResult result)
    {
        var operation = result.Operation
            ?? throw MissingCheckoutResult(nameof(result.Operation));
        return TypedResults.Json(
            new BillingCheckoutInProgressErrorResponse(
                BillingCheckoutReasonCodes.OperationInProgress,
                operation.Id,
                CadenceName(operation.Cadence),
                operation.SeatQuantity,
                operation.ExpiresAt),
            statusCode: StatusCodes.Status409Conflict);
    }

    private static JsonHttpResult<BillingCheckoutCapacityChangedErrorResponse>
        CapacityChangedError(BillingCheckoutResult result)
    {
        var operation = result.Operation
            ?? throw MissingCheckoutResult(nameof(result.Operation));
        return TypedResults.Json(
            new BillingCheckoutCapacityChangedErrorResponse(
                BillingCheckoutReasonCodes.OperationCapacityChanged,
                operation.Id,
                CadenceName(operation.Cadence),
                operation.SeatQuantity,
                result.RequiredSeatQuantity
                    ?? throw MissingCheckoutResult(nameof(result.RequiredSeatQuantity)),
                operation.ExpiresAt),
            statusCode: StatusCodes.Status409Conflict);
    }

    private static string CadenceName(BillingCadence cadence)
    {
        return cadence switch
        {
            BillingCadence.Monthly => "monthly",
            BillingCadence.Annual => "annual",
            _ => throw new ArgumentOutOfRangeException(nameof(cadence)),
        };
    }

    private static JsonHttpResult<BillingCapacityErrorResponse> CapacityError(
        string reasonCode,
        BillingCheckoutResult result)
    {
        return TypedResults.Json(
            new BillingCapacityErrorResponse(
                reasonCode,
                result.RequiredSeatQuantity
                    ?? throw MissingCheckoutResult(nameof(result.RequiredSeatQuantity))),
            statusCode: StatusCodes.Status409Conflict);
    }

    private static JsonHttpResult<BillingCapacityErrorResponse> CapacityError(
        string reasonCode,
        int? requiredSeatQuantity)
    {
        return TypedResults.Json(
            new BillingCapacityErrorResponse(
                reasonCode,
                requiredSeatQuantity
                    ?? throw MissingSeatQuantityResult(nameof(requiredSeatQuantity))),
            statusCode: StatusCodes.Status409Conflict);
    }

    private static JsonHttpResult<BillingSeatQuantityOperationErrorResponse>
        SeatQuantityOperationError(
            int statusCode,
            string reasonCode,
            BillingSeatQuantityResult result)
    {
        var operation = result.Operation
            ?? throw MissingSeatQuantityResult(nameof(result.Operation));
        return TypedResults.Json(
            new BillingSeatQuantityOperationErrorResponse(
                reasonCode,
                operation.Id,
                operation.PreviousSeatQuantity,
                operation.SeatQuantity),
            statusCode: statusCode);
    }

    private static JsonHttpResult<BillingErrorResponse> BillingError(
        int statusCode,
        string reasonCode)
    {
        return TypedResults.Json(
            new BillingErrorResponse(reasonCode),
            statusCode: statusCode);
    }

    private static InvalidOperationException MissingCheckoutResult(string propertyName)
    {
        return new($"The Checkout result is missing {propertyName}.");
    }

    private static InvalidOperationException MissingCustomerPortalResult(
        string propertyName)
    {
        return new($"The Customer Portal result is missing {propertyName}.");
    }

    private static InvalidOperationException MissingSeatQuantityResult(
        string propertyName)
    {
        return new($"The seat-quantity result is missing {propertyName}.");
    }
}
