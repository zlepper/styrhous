using Styrhous.Licensing.Persistence;
namespace Styrhous.Licensing.Application.Billing;



public sealed class BillingCheckoutCompletionReconciler(
    ICommercialSubscriptionProvider subscriptionProvider,
    PostgresBillingCheckoutStore checkoutStore)

{

    public async Task ReconcileAsync(
        Guid expectedBillingAccountId,
        Guid expectedBillingOperationId,
        string externalSubscriptionId,
        CancellationToken cancellationToken)
    {
        var authoritative = await subscriptionProvider.ResolveCheckoutSubscriptionAsync(
            externalSubscriptionId,
            expectedBillingOperationId,
            cancellationToken);
        if (authoritative.BillingAccountId != expectedBillingAccountId)
        {
            throw new InvalidOperationException(
                "The completed Checkout subscription belongs to a different billing account.");
        }

        if (!await checkoutStore.ApplySubscriptionAndCompleteProviderSessionAsync(
                expectedBillingOperationId,
                authoritative,
                cancellationToken))
        {
            throw new InvalidOperationException(
                "The completed Checkout operation could not be closed.");
        }
    }
}
