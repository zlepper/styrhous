namespace Styrhous.Licensing.Api.Billing;

public static class BillingSeatQuantityReasonCodes
{
    public const string Changed = "seat_quantity_changed";

    public const string BillingAccountNotFound = BillingReasonCodes.BillingAccountNotFound;

    public const string BillingOperationNotFound = BillingReasonCodes.BillingOperationNotFound;

    public const string InsufficientPermission = BillingReasonCodes.InsufficientPermission;

    public const string PersonalSeatQuantityInvalid =
        BillingReasonCodes.PersonalSeatQuantityInvalid;

    public const string SeatQuantityTooSmall =
        BillingReasonCodes.SeatQuantityTooSmall;

    public const string SeatQuantityUnchanged = "seat_quantity_unchanged";

    public const string SubscriptionNotFound = BillingReasonCodes.SubscriptionNotFound;

    public const string SubscriptionInactive = "subscription_inactive";

    public const string OperationInProgress = "seat_quantity_operation_in_progress";

    public const string SubscriptionQuantityChanged = "subscription_quantity_changed";

    public const string ProviderUnavailable = "seat_quantity_provider_unavailable";

    public const string ProviderReconciliationRequired =
        "seat_quantity_reconciliation_required";
}
