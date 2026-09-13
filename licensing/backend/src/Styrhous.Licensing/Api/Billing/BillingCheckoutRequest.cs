using System.ComponentModel.DataAnnotations;
using Styrhous.Licensing.Api.Validation;

namespace Styrhous.Licensing.Api.Billing;

public sealed record BillingCheckoutRequest(
    [property: Required, NormalizedAllowedValues("monthly", "annual")] string? Cadence,
    [property: Range(1, int.MaxValue)] int SeatQuantity,
    [property: Uuid] string? BillingOperationId);
