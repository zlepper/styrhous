using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public enum BillingCheckoutStatus
{
    Created,
    BillingAccountNotFound,
    BillingOperationNotFound,
    InsufficientPermission,
    PersonalSeatQuantityInvalid,
    SeatQuantityTooSmall,
    SubscriptionAlreadyExists,
    CheckoutOperationInProgress,
    CheckoutOperationCapacityChanged,
    ProviderUnavailable,
}

public sealed record BillingCheckoutAttempt(
    Guid Id,
    BillingCadence Cadence,
    int SeatQuantity,
    DateTimeOffset ExpiresAt);

public sealed record BillingCheckoutResult(
    BillingCheckoutStatus Status,
    int? RequiredSeatQuantity,
    BillingCheckoutAttempt? Operation,
    Uri? RedirectUri)
{
    public Guid? BillingOperationId => Operation?.Id;
}
