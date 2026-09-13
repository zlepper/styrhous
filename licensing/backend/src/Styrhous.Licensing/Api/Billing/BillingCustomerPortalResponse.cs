namespace Styrhous.Licensing.Api.Billing;

public sealed record BillingCustomerPortalResponse(
    string ReasonCode,
    string RedirectUrl);
