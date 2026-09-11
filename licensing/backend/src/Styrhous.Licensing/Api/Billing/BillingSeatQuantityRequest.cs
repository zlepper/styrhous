using System.ComponentModel.DataAnnotations;
using Styrhous.Licensing.Api.Validation;

namespace Styrhous.Licensing.Api.Billing;

public sealed record BillingSeatQuantityRequest(
    [property: Range(1, int.MaxValue)] int SeatQuantity,
    [property: Uuid] string? BillingOperationId);
