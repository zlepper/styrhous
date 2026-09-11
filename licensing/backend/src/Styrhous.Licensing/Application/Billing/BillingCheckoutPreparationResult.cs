using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public enum BillingCheckoutPreparationStatus
{
    Prepared,
    BillingAccountNotFound,
    BillingOperationNotFound,
    InsufficientPermission,
    PersonalSeatQuantityInvalid,
    SeatQuantityTooSmall,
    SubscriptionAlreadyExists,
    CheckoutOperationInProgress,
    CheckoutOperationCapacityChanged,
    ProviderSessionReconciliationRequired,
}

public sealed record BillingCheckoutPreparationResult(
    BillingCheckoutPreparationStatus Status,
    int? RequiredSeatQuantity,
    BillingOperation? Operation)
{
    public static BillingCheckoutPreparationResult Prepared(
        BillingOperation operation,
        int requiredSeatQuantity)
    {
        return new(
            BillingCheckoutPreparationStatus.Prepared,
            requiredSeatQuantity,
            operation);
    }

    public static BillingCheckoutPreparationResult Rejected(
        BillingCheckoutPreparationStatus status,
        int? requiredSeatQuantity = null)
    {
        if (status is BillingCheckoutPreparationStatus.Prepared
            or BillingCheckoutPreparationStatus.CheckoutOperationInProgress
            or BillingCheckoutPreparationStatus.CheckoutOperationCapacityChanged
            or BillingCheckoutPreparationStatus.ProviderSessionReconciliationRequired)
        {
            throw new ArgumentException(
                "A rejected Checkout preparation cannot have Prepared status.",
                nameof(status));
        }

        return new BillingCheckoutPreparationResult(status, requiredSeatQuantity, null);
    }

    public static BillingCheckoutPreparationResult InProgress(
        BillingOperation operation,
        int requiredSeatQuantity)
    {
        return new(
            BillingCheckoutPreparationStatus.CheckoutOperationInProgress,
            requiredSeatQuantity,
            operation);
    }

    public static BillingCheckoutPreparationResult CapacityChanged(
        BillingOperation operation,
        int requiredSeatQuantity)
    {
        return new(
            BillingCheckoutPreparationStatus.CheckoutOperationCapacityChanged,
            requiredSeatQuantity,
            operation);
    }

    public static BillingCheckoutPreparationResult ReconciliationRequired(
        BillingOperation operation,
        int requiredSeatQuantity)
    {
        return new(
            BillingCheckoutPreparationStatus.ProviderSessionReconciliationRequired,
            requiredSeatQuantity,
            operation);
    }
}
