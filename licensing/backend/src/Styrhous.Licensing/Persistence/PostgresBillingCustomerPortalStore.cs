using System.Data;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresBillingCustomerPortalStore(LicensingDbContext dbContext)

{

    public async Task<BillingCustomerPortalPreparationResult> PrepareAsync(
        Guid actorUserId,
        Guid billingAccountId,
        CancellationToken cancellationToken)
    {
        return await LicensingDbContextTransaction.ExecuteAsync<
            BillingCustomerPortalPreparationResult>(
            dbContext,
            IsolationLevel.RepeatableRead,
            async (transaction, token) =>
            {
                await PostgresReadChecks.EnsureUserExistsAsync(
                    dbContext,
                    actorUserId,
                    token);

                var account = await AccountForActor(actorUserId, billingAccountId)
                    .SingleOrDefaultAsync(cancellationToken);
                if (account is null)
                {
                    return Reject(BillingCustomerPortalPreparationStatus.BillingAccountNotFound);
                }

                var authorization = BillingAccountManagementPolicy.Authorize(
                    actorUserId,
                    account.Kind,
                    account.PersonalOwnerUserId,
                    account.OrganizationRole);
                if (authorization != BillingAccountManagementAuthorization.Authorized)
                {
                    return Reject(authorization switch
                    {
                        BillingAccountManagementAuthorization.BillingAccountNotFound =>
                            BillingCustomerPortalPreparationStatus.BillingAccountNotFound,
                        BillingAccountManagementAuthorization.InsufficientPermission =>
                            BillingCustomerPortalPreparationStatus.InsufficientPermission,
                        _ => throw new InvalidOperationException(
                            "The billing-management authorization result is unsupported."),
                    });
                }

                if (account.ExternalCustomerId is null)
                {
                    return Reject(BillingCustomerPortalPreparationStatus.SubscriptionNotFound);
                }

                await transaction.CommitAsync(token);
                return BillingCustomerPortalPreparationResult.Prepared(account.ExternalCustomerId);
            },
            cancellationToken);
    }

    private IQueryable<BillingCustomerPortalAccount> AccountForActor(
        Guid actorUserId,
        Guid billingAccountId)
    {
        return from account in dbContext.BillingAccounts.AsNoTracking()
               join organization in dbContext.Organizations.AsNoTracking()
                   on account.Id equals organization.BillingAccountId into organizations
               from organization in organizations.DefaultIfEmpty()
               join membership in dbContext.OrganizationMemberships
                       .AsNoTracking()
                       .Where(candidate => candidate.UserId == actorUserId)
                   on organization.Id equals membership.OrganizationId into memberships
               from membership in memberships.DefaultIfEmpty()
               join subscription in dbContext.CommercialSubscriptions.AsNoTracking()
                   on account.Id equals subscription.BillingAccountId into subscriptions
               from subscription in subscriptions.DefaultIfEmpty()
               where account.Id == billingAccountId
               select new BillingCustomerPortalAccount(
                   account.Kind,
                   account.PersonalOwnerUserId,
                   membership == null ? null : membership.Role,
                   subscription == null ? null : subscription.ExternalCustomerId);
    }

    private static BillingCustomerPortalPreparationResult Reject(
        BillingCustomerPortalPreparationStatus status)
    {
        return BillingCustomerPortalPreparationResult.Rejected(status);
    }

    private sealed record BillingCustomerPortalAccount(
        BillingAccountKind Kind,
        Guid? PersonalOwnerUserId,
        OrganizationRole? OrganizationRole,
        string? ExternalCustomerId);
}
