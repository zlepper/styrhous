namespace Styrhous.Licensing.Api.Billing;

public static class BillingReasonCodes
{
    public const string BillingAccountNotFound = "billing_account_not_found";

    public const string BillingOperationNotFound = "billing_operation_not_found";

    public const string InsufficientPermission = "insufficient_permission";

    public const string SubscriptionNotFound = "subscription_not_found";

    public const string PersonalSeatQuantityInvalid =
        "personal_seat_quantity_invalid";

    public const string SeatQuantityTooSmall = "seat_quantity_too_small";
}
