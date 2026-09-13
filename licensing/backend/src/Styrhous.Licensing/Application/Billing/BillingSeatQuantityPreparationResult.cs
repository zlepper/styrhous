using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public enum BillingSeatQuantityPreparationStatus
{
    Prepared,
    BillingAccountNotFound,
    BillingOperationNotFound,
    InsufficientPermission,
    PersonalSeatQuantityInvalid,
    SeatQuantityTooSmall,
    SeatQuantityUnchanged,
    SubscriptionNotFound,
    SubscriptionInactive,
    OperationInProgress,
    Completed,
    Failed,
}

public sealed record BillingSeatQuantityPreparationResult(
    BillingSeatQuantityPreparationStatus Status,
    BillingOperation? Operation,
    string? ExternalSubscriptionId,
    int? RequiredSeatQuantity,
    int? AuthoritativeSeatQuantity)
{
    public static BillingSeatQuantityPreparationResult Prepared(
        BillingOperation operation,
        string externalSubscriptionId,
        int requiredSeatQuantity)
    {
        return new(
            BillingSeatQuantityPreparationStatus.Prepared,
            operation,
            externalSubscriptionId,
            requiredSeatQuantity,
            AuthoritativeSeatQuantity: null);
    }

    public static BillingSeatQuantityPreparationResult InProgress(
        BillingOperation operation,
        int requiredSeatQuantity)
    {
        return new(
            BillingSeatQuantityPreparationStatus.OperationInProgress,
            operation,
            ExternalSubscriptionId: null,
            requiredSeatQuantity,
            AuthoritativeSeatQuantity: null);
    }

    public static BillingSeatQuantityPreparationResult Terminal(
        BillingOperation operation,
        int authoritativeSeatQuantity,
        int requiredSeatQuantity)
    {
        if (operation.Status is not BillingOperationStatus.Completed
            and not BillingOperationStatus.Failed)
        {
            throw new ArgumentException(
                "A terminal seat-quantity preparation requires a closed operation.",
                nameof(operation));
        }

        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(authoritativeSeatQuantity);
        return new BillingSeatQuantityPreparationResult(
            operation.Status == BillingOperationStatus.Completed
                ? BillingSeatQuantityPreparationStatus.Completed
                : BillingSeatQuantityPreparationStatus.Failed,
            operation,
            ExternalSubscriptionId: null,
            requiredSeatQuantity,
            authoritativeSeatQuantity);
    }

    public static BillingSeatQuantityPreparationResult Rejected(
        BillingSeatQuantityPreparationStatus status,
        int? requiredSeatQuantity = null)
    {
        if (status is BillingSeatQuantityPreparationStatus.Prepared
            or BillingSeatQuantityPreparationStatus.OperationInProgress
            or BillingSeatQuantityPreparationStatus.Completed
            or BillingSeatQuantityPreparationStatus.Failed)
        {
            throw new ArgumentException(
                "A rejected seat-quantity preparation has an invalid status.",
                nameof(status));
        }

        return new BillingSeatQuantityPreparationResult(
            status,
            Operation: null,
            ExternalSubscriptionId: null,
            requiredSeatQuantity,
            AuthoritativeSeatQuantity: null);
    }
}
