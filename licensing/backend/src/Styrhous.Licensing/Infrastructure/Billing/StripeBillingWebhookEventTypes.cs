using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Infrastructure.Billing;

public static class StripeBillingWebhookEventTypes
{
    public const string CheckoutSessionCompleted = "checkout.session.completed";

    public const string CustomerSubscriptionCreated = "customer.subscription.created";

    public const string CustomerSubscriptionUpdated = "customer.subscription.updated";

    public const string CustomerSubscriptionDeleted = "customer.subscription.deleted";

    public const string InvoicePaid = "invoice.paid";

    public const string InvoicePaymentFailed = "invoice.payment_failed";

    public static BillingWebhookEventKind ToKind(string eventType)
    {
        return eventType switch
        {
            CheckoutSessionCompleted => BillingWebhookEventKind.CheckoutCompleted,
            CustomerSubscriptionCreated
                or CustomerSubscriptionUpdated
                or CustomerSubscriptionDeleted => BillingWebhookEventKind.SubscriptionChanged,
            InvoicePaid => BillingWebhookEventKind.InvoicePaid,
            InvoicePaymentFailed => BillingWebhookEventKind.PaymentFailed,
            _ => BillingWebhookEventKind.Unsupported,
        };
    }
}
