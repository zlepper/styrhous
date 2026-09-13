using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Billing;

public enum BillingAccountManagementAuthorization
{
    Authorized,
    BillingAccountNotFound,
    InsufficientPermission,
}

public static class BillingAccountManagementPolicy
{
    public static BillingAccountManagementAuthorization Authorize(
        Guid actorUserId,
        BillingAccountKind accountKind,
        Guid? personalOwnerUserId,
        OrganizationRole? organizationRole)
    {
        if (accountKind == BillingAccountKind.Personal)
        {
            return personalOwnerUserId == actorUserId
                ? BillingAccountManagementAuthorization.Authorized
                : BillingAccountManagementAuthorization.BillingAccountNotFound;
        }

        if (accountKind != BillingAccountKind.Organization)
        {
            throw new ArgumentOutOfRangeException(nameof(accountKind));
        }

        if (organizationRole is null)
        {
            return BillingAccountManagementAuthorization.BillingAccountNotFound;
        }

        return organizationRole == OrganizationRole.Owner
            ? BillingAccountManagementAuthorization.Authorized
            : BillingAccountManagementAuthorization.InsufficientPermission;
    }
}
