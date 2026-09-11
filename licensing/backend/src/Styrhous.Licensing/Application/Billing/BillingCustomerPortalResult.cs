namespace Styrhous.Licensing.Application.Billing;

public enum BillingCustomerPortalStatus
{
    Created,
    BillingAccountNotFound,
    InsufficientPermission,
    SubscriptionNotFound,
    ProviderUnavailable,
}

public sealed record BillingCustomerPortalResult(
    BillingCustomerPortalStatus Status,
    Uri? RedirectUri);
