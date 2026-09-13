using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public enum BillingSeatQuantityStatus
{
    Changed,
    BillingAccountNotFound,
    BillingOperationNotFound,
    InsufficientPermission,
    PersonalSeatQuantityInvalid,
    SeatQuantityTooSmall,
    SeatQuantityUnchanged,
    SubscriptionNotFound,
    SubscriptionInactive,
    OperationInProgress,
    SubscriptionQuantityChanged,
    ProviderUnavailable,
    ProviderReconciliationRequired,
}

public sealed record BillingSeatQuantityOperation(
    Guid Id,
    int PreviousSeatQuantity,
    int SeatQuantity);

public sealed record BillingSeatQuantityResult(
    BillingSeatQuantityStatus Status,
    BillingSeatQuantityOperation? Operation,
    int? RequiredSeatQuantity,
    int? AuthoritativeSeatQuantity)
{
    public Guid? BillingOperationId => Operation?.Id;
}

public sealed record BillingSeatQuantityResolutionResult(
    BillingOperationStatus OperationStatus,
    SeatQuantityChangeOutcome? Outcome,
    int AuthoritativeSeatQuantity);

public sealed record BillingSeatQuantityMutationPreparationResult(
    BillingOperationStatus OperationStatus,
    SeatQuantityChangeOutcome? Outcome,
    DateTimeOffset? ProviderMutationReplayStartedAt,
    DateTimeOffset AuthoritativeProviderObservedAt,
    int AuthoritativeSeatQuantity);
