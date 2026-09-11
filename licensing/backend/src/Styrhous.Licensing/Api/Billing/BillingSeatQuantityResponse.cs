namespace Styrhous.Licensing.Api.Billing;

public sealed record BillingSeatQuantityResponse(
    string ReasonCode,
    Guid BillingOperationId,
    int SeatQuantity);

public sealed record BillingSeatQuantityOperationErrorResponse(
    string ReasonCode,
    Guid BillingOperationId,
    int PreviousSeatQuantity,
    int SeatQuantity);

public sealed record BillingSeatQuantityStateErrorResponse(
    string ReasonCode,
    int SeatQuantity);
