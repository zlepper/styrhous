using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public interface ICommercialSubscriptionProvider
{
    Task<AuthoritativeCommercialSubscription?> ResolveEventAsync(
        string externalEventId,
        BillingWebhookEventKind kind,
        CancellationToken cancellationToken);

    Task<AuthoritativeCommercialSubscription> ResolveCheckoutSubscriptionAsync(
        string externalSubscriptionId,
        Guid expectedBillingOperationId,
        CancellationToken cancellationToken);
}
