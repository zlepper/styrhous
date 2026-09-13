namespace Styrhous.Licensing.Application.Entitlements;

public static class EntitlementReasonCodes
{
    public const string NoValidEntitlement = "no_valid_entitlement";
    public const string ProductSeatNotAssigned = "product_seat_not_assigned";
    public const string ActiveTrial = "active_trial";
    public const string TrialNotStarted = "trial_not_started";
    public const string TrialExpired = "trial_expired";
    public const string ActiveSubscription = "active_subscription";
    public const string SubscriptionPastDue = "subscription_past_due";
    public const string SubscriptionCancelsAtPeriodEnd =
        "subscription_cancels_at_period_end";
    public const string SubscriptionNotStarted = "subscription_not_started";
    public const string SubscriptionExpired = "subscription_expired";
    public const string SubscriptionRenewalPending = "subscription_renewal_pending";
    public const string SubscriptionInactive = "subscription_inactive";
    public const string SubscriptionSeatCapacityExceeded =
        "subscription_seat_capacity_exceeded";
}
