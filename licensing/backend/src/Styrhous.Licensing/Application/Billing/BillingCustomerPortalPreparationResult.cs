using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Application.Billing;

public enum BillingCustomerPortalPreparationStatus
{
    Prepared,
    BillingAccountNotFound,
    InsufficientPermission,
    SubscriptionNotFound,
}

public sealed record BillingCustomerPortalPreparationResult
{
    private BillingCustomerPortalPreparationResult(
        BillingCustomerPortalPreparationStatus status,
        string? externalCustomerId)
    {
        Status = status;
        ExternalCustomerId = externalCustomerId;
    }

    public BillingCustomerPortalPreparationStatus Status { get; }

    public string? ExternalCustomerId { get; }

    public static BillingCustomerPortalPreparationResult Prepared(
        string externalCustomerId)
    {
        return new(
            BillingCustomerPortalPreparationStatus.Prepared,
            RequiredText.Normalize(
                externalCustomerId,
                nameof(externalCustomerId),
                CommercialSubscription.MaximumExternalIdentifierLength,
                "external customer identifier"));
    }

    public static BillingCustomerPortalPreparationResult Rejected(
        BillingCustomerPortalPreparationStatus status)
    {
        if (status == BillingCustomerPortalPreparationStatus.Prepared)
        {
            throw new ArgumentException(
                "A prepared Customer Portal result requires an external customer.",
                nameof(status));
        }

        return new BillingCustomerPortalPreparationResult(
            status,
            externalCustomerId: null);
    }
}
