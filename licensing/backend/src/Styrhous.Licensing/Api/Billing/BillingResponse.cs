namespace Styrhous.Licensing.Api.Billing;

public sealed record BillingErrorResponse(string ReasonCode);

public sealed record BillingCapacityErrorResponse(
    string ReasonCode,
    int RequiredSeatQuantity);
