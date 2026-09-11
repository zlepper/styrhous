using Styrhous.Licensing.Domain.Accounts;

namespace Styrhous.Licensing.Persistence;

internal static class PostgresBillingSeatRequirement
{
    public static async Task<int> ResolveAsync(
        LicensingDbContext dbContext,
        BillingAccountKind accountKind,
        Guid? organizationId,
        Guid billingAccountId,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        if (accountKind == BillingAccountKind.Personal)
        {
            return 1;
        }

        if (accountKind != BillingAccountKind.Organization)
        {
            throw new InvalidOperationException(
                "The billing account kind cannot determine a seat requirement.");
        }

        return await PostgresOrganizationInvitationCapacity.RequiredSeatCountAsync(
            dbContext,
            organizationId
                ?? throw new InvalidOperationException(
                    "An organization billing account must have an organization."),
            billingAccountId,
            observedAt,
            excludedInvitationId: null,
            cancellationToken);
    }
}
