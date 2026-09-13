namespace Styrhous.Licensing.Api.Billing;

public static class BillingCheckoutReasonCodes
{
    public const string SessionCreated = "checkout_session_created";

    public const string BillingAccountNotFound = BillingReasonCodes.BillingAccountNotFound;

    public const string BillingOperationNotFound = BillingReasonCodes.BillingOperationNotFound;

    public const string InsufficientPermission = BillingReasonCodes.InsufficientPermission;

    public const string PersonalSeatQuantityInvalid =
        BillingReasonCodes.PersonalSeatQuantityInvalid;

    public const string SeatQuantityTooSmall = BillingReasonCodes.SeatQuantityTooSmall;

    public const string SubscriptionAlreadyExists = "subscription_already_exists";

    public const string OperationInProgress = "checkout_operation_in_progress";

    public const string OperationCapacityChanged =
        "checkout_operation_capacity_changed";

    public const string ProviderUnavailable = "checkout_provider_unavailable";
}
