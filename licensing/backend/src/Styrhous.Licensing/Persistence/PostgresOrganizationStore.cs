using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresOrganizationStore(LicensingDbContext dbContext)
{

    public async Task<OrganizationCreationResult> AddAsync(
        OrganizationRegistration registration,
        Guid correlationId,
        CancellationToken cancellationToken)
    {
        return await LicensingDbContextTransaction.ExecuteAsync<OrganizationCreationResult>(
            dbContext,
            async (transaction, token) =>
        {
            var ownerUserId = registration.OwnerMembership.UserId;
            if (!await EfTransactionSerialization.TryClaimUserAsync(
                    dbContext,
                    ownerUserId,
                    cancellationToken))
            {
                throw new UserNotFoundException();
            }

            var hasCreatedOrganization = await dbContext.Organizations
                .AnyAsync(
                    organization => organization.CreatedByUserId == ownerUserId,
                    cancellationToken);
            var trial = await dbContext.Trials
                .SingleOrDefaultAsync(
                    candidate => candidate.OriginatingUserId == ownerUserId,
                    cancellationToken);
            var trialIsStillPersonal = trial is not null
                && await dbContext.BillingAccounts.AnyAsync(
                    account => account.Id == trial.BillingAccountId
                        && account.Kind == BillingAccountKind.Personal,
                    cancellationToken);
            var trialWasTransferred = !hasCreatedOrganization
                && trialIsStillPersonal
                && trial!.TryTransferToFirstOrganization(
                    registration.BillingAccount.Id,
                    registration.ObservedAt);

            dbContext.AddRange(
                registration.BillingAccount,
                registration.Organization,
                registration.OwnerMembership,
                registration.Seat);
            dbContext.AuditRecords.AddRange(
                OrganizationCreationAuditRecords.Create(
                    registration,
                    correlationId,
                    trialWasTransferred ? trial : null));

            await dbContext.SaveChangesAsync(cancellationToken);
            await transaction.CommitAsync(cancellationToken);
            return OrganizationCreationResult.Created(
                registration,
                correlationId,
                trialWasTransferred);
        },
            cancellationToken);
    }
}
