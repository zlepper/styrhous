namespace Styrhous.Licensing.Api.Billing;

public sealed record BillingCheckoutResponse(
    string ReasonCode,
    Guid BillingOperationId,
    string RedirectUrl);

public sealed record BillingCheckoutProviderErrorResponse(
    string ReasonCode,
    Guid BillingOperationId);

public sealed record BillingCheckoutInProgressErrorResponse(
    string ReasonCode,
    Guid BillingOperationId,
    string Cadence,
    int SeatQuantity,
    DateTimeOffset ExpiresAt);

public sealed record BillingCheckoutCapacityChangedErrorResponse(
    string ReasonCode,
    Guid BillingOperationId,
    string Cadence,
    int SeatQuantity,
    int RequiredSeatQuantity,
    DateTimeOffset ExpiresAt);
