using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Billing;

public sealed class BillingCustomerPortalService(
    PostgresBillingCustomerPortalStore store,
    IBillingCustomerPortalProvider provider)
{

    public async Task<BillingCustomerPortalResult> CreateSessionAsync(
        Guid actorUserId,
        Guid billingAccountId,
        CancellationToken cancellationToken = default)
    {

        var preparation = await store.PrepareAsync(
            actorUserId,
            billingAccountId,
            cancellationToken);
        if (preparation.Status != BillingCustomerPortalPreparationStatus.Prepared)
        {
            return new BillingCustomerPortalResult(
                Map(preparation.Status),
                RedirectUri: null);
        }

        BillingCustomerPortalProviderSession session;
        try
        {
            session = await provider.CreateSessionAsync(
                preparation.ExternalCustomerId
                    ?? throw new InvalidOperationException(
                        "A prepared Customer Portal account requires an external customer."),
                cancellationToken);
        }
        catch (BillingCustomerPortalProviderUnavailableException)
        {
            return new BillingCustomerPortalResult(
                BillingCustomerPortalStatus.ProviderUnavailable,
                RedirectUri: null);
        }

        if (!session.RedirectUri.IsAbsoluteUri
            || !string.Equals(
                session.RedirectUri.Scheme,
                Uri.UriSchemeHttps,
                StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                "The billing provider returned an invalid Customer Portal redirect URI.");
        }

        return new BillingCustomerPortalResult(
            BillingCustomerPortalStatus.Created,
            session.RedirectUri);
    }

    private static BillingCustomerPortalStatus Map(
        BillingCustomerPortalPreparationStatus status)
    {
        return status switch
        {
            BillingCustomerPortalPreparationStatus.BillingAccountNotFound =>
                BillingCustomerPortalStatus.BillingAccountNotFound,
            BillingCustomerPortalPreparationStatus.InsufficientPermission =>
                BillingCustomerPortalStatus.InsufficientPermission,
            BillingCustomerPortalPreparationStatus.SubscriptionNotFound =>
                BillingCustomerPortalStatus.SubscriptionNotFound,
            BillingCustomerPortalPreparationStatus.Prepared =>
                throw new ArgumentOutOfRangeException(nameof(status)),
            _ => throw new ArgumentOutOfRangeException(nameof(status)),
        };
    }
}
