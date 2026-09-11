using System.Data;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Persistence;

public sealed class PostgresBillingAccountListingStore(LicensingDbContext dbContext)

{

    public async Task<IReadOnlyList<BillingAccountSummary>> ListForUserAsync(
        Guid userId,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        return await LicensingDbContextTransaction.ExecuteAsync<
            IReadOnlyList<BillingAccountSummary>>(
            dbContext,
            IsolationLevel.RepeatableRead,
            async (transaction, token) =>
            {
                await PostgresReadChecks.EnsureUserExistsAsync(
                    dbContext,
                    userId,
                    token);

                var seatStatesByAccount = (await SeatEntitlementQuery.LoadForUserAsync(
                        dbContext,
                        userId,
                        cancellationToken))
                    .ToDictionary(
                        item => item.Source.BillingAccountId,
                        item => new BillingAccountSeatState(
                            item.Row,
                            SeatEntitlement.Resolve(item.Source, observedAt)));

                var rows = await AccountQuery(userId)
                    .ToArrayAsync(cancellationToken);
                if (rows.Length == 0 || rows[0].AccountKind != BillingAccountKind.Personal)
                {
                    throw new InvalidOperationException(
                        "A persisted user must have one personal billing account.");
                }

                var result = rows
                    .Select(row => CreateSummary(row, seatStatesByAccount, observedAt))
                    .ToArray();
                await transaction.CommitAsync(token);
                return result;
            },
            cancellationToken);
    }

    private IQueryable<BillingAccountListingRow> AccountQuery(Guid userId)
    {
        return from account in dbContext.BillingAccounts.AsNoTracking()
               join organization in dbContext.Organizations.AsNoTracking()
                   on account.Id equals organization.BillingAccountId into organizations
               from organization in organizations.DefaultIfEmpty()
               join membership in dbContext.OrganizationMemberships
                       .AsNoTracking()
                       .Where(candidate => candidate.UserId == userId)
                   on organization.Id equals membership.OrganizationId into memberships
               from membership in memberships.DefaultIfEmpty()
               join trial in dbContext.Trials.AsNoTracking()
                   on account.Id equals trial.BillingAccountId into trials
               from trial in trials.DefaultIfEmpty()
               join subscription in dbContext.CommercialSubscriptions.AsNoTracking()
                   on account.Id equals subscription.BillingAccountId into subscriptions
               from subscription in subscriptions.DefaultIfEmpty()
               where (account.Kind == BillingAccountKind.Personal
                       && account.PersonalOwnerUserId == userId)
                   || (account.Kind == BillingAccountKind.Organization && membership != null)
               orderby account.Kind == BillingAccountKind.Personal ? 0 : 1,
                   membership.CreatedAt,
                   membership.Id
               select new BillingAccountListingRow(
                   account.Id,
                   account.Kind,
                   organization == null ? null : organization.Id,
                   organization == null ? null : organization.Name,
                   membership == null ? null : membership.Role,
                   dbContext.Seats.Count(seat => seat.BillingAccountId == account.Id
                       && seat.ProductAccessEnabled),
                   trial == null ? null : trial.Id,
                   trial == null ? null : trial.TransferredAt,
                   subscription == null ? null : subscription.Id,
                   subscription == null ? null : subscription.ProjectedAt);
    }

    private static BillingAccountSummary CreateSummary(
        BillingAccountListingRow row,
        Dictionary<Guid, BillingAccountSeatState> seatStatesByAccount,
        DateTimeOffset observedAt)
    {
        if (!seatStatesByAccount.TryGetValue(row.BillingAccountId, out var seatState))
        {
            throw new InvalidOperationException(
                "Every listed billing account must have a seat assigned to the current user.");
        }

        var source = seatState.Row;
        var entitlement = seatState.Entitlement;

        return new BillingAccountSummary(
            row.BillingAccountId,
            row.AccountKind,
            row.OrganizationId,
            row.OrganizationName,
            row.OrganizationRole,
            row.AssignedSeatCount,
            entitlement,
            row.TrialId is null
                ? null
                : new BillingTrialSummary(
                    row.TrialId.Value,
                    source.TrialStartedAt!.Value,
                    source.TrialEndsAt!.Value,
                    source.TrialTerminatedAt,
                    row.TrialTransferredAt,
                    observedAt >= source.TrialStartedAt.Value
                        && observedAt < (source.TrialTerminatedAt
                            ?? source.TrialEndsAt.Value)),
            row.SubscriptionId is null
                ? null
                : new BillingSubscriptionSummary(
                    row.SubscriptionId.Value,
                    source.SubscriptionStatus!.Value,
                    source.SubscriptionSeatQuantity!.Value,
                    source.SubscriptionCancelsAtPeriodEnd,
                    source.SubscriptionPeriodStartedAt!.Value,
                    source.SubscriptionPeriodEndsAt!.Value,
                    row.SubscriptionProjectedAt!.Value));
    }

    private sealed record BillingAccountSeatState(
        SeatEntitlementRow Row,
        SeatEntitlement Entitlement);

    private sealed record BillingAccountListingRow(
        Guid BillingAccountId,
        BillingAccountKind AccountKind,
        Guid? OrganizationId,
        string? OrganizationName,
        OrganizationRole? OrganizationRole,
        int AssignedSeatCount,
        Guid? TrialId,
        DateTimeOffset? TrialTransferredAt,
        Guid? SubscriptionId,
        DateTimeOffset? SubscriptionProjectedAt);
}
