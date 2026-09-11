namespace Styrhous.Licensing.Domain.Billing;

public static class BillingCheckoutEligibility
{
    public static bool AllowsPurchase(CommercialSubscriptionStatus? status)
    {
        return status is null or CommercialSubscriptionStatus.Canceled or CommercialSubscriptionStatus.IncompleteExpired;
    }
}
