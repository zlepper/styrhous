using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Persistence;

internal sealed record SeatEntitlementRow(
    Guid SeatId,
    Guid BillingAccountId,
    bool ProductAccessEnabled,
    DateTimeOffset? TrialStartedAt,
    DateTimeOffset? TrialEndsAt,
    DateTimeOffset? TrialTerminatedAt,
    CommercialSubscriptionStatus? SubscriptionStatus,
    DateTimeOffset? SubscriptionPeriodStartedAt,
    DateTimeOffset? SubscriptionPeriodEndsAt,
    bool SubscriptionCancelsAtPeriodEnd,
    int? SubscriptionSeatQuantity)
{
    public SeatEntitlementSource ToSource(bool subscriptionSeatFunded)
    {
        CommercialSeatEntitlementSource? commercial = null;
        if (SubscriptionStatus is not null)
        {
            commercial = new CommercialSeatEntitlementSource(
                SubscriptionStatus.Value,
                SubscriptionPeriodStartedAt
                    ?? throw new InvalidOperationException(
                        "A persisted subscription must have a period start."),
                SubscriptionPeriodEndsAt
                    ?? throw new InvalidOperationException(
                        "A persisted subscription must have a period end."),
                SubscriptionCancelsAtPeriodEnd,
                subscriptionSeatFunded);
        }

        return new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            TrialStartedAt,
            TrialTerminatedAt ?? TrialEndsAt,
            commercial,
            ProductAccessEnabled);
    }
}

internal sealed record SeatEntitlementMaterialization(
    SeatEntitlementRow Row,
    SeatEntitlementSource Source);

internal static class SeatEntitlementQuery
{
    public static async Task<IReadOnlyList<SeatEntitlementMaterialization>> LoadForUserAsync(
        LicensingDbContext dbContext,
        Guid userId,
        CancellationToken cancellationToken)
    {
        var rows = await ForUser(dbContext, userId).ToArrayAsync(cancellationToken);
        var fundedSeatIds = await CommercialSeatFundingQuery.ResolveFundedSeatIdsAsync(
            dbContext,
            rows,
            cancellationToken);
        return rows
            .Select(row => new SeatEntitlementMaterialization(
                row,
                row.ToSource(
                    row.SubscriptionStatus is null || fundedSeatIds.Contains(row.SeatId))))
            .ToArray();
    }

    public static async Task<SeatEntitlement> SerializeBillingAccountAndResolveForSeatAsync(
        LicensingDbContext dbContext,
        Guid seatId,
        Guid billingAccountId,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        if (!await EfTransactionSerialization.TryClaimBillingAccountAsync(
                dbContext,
                billingAccountId,
                cancellationToken))
        {
            throw new InvalidOperationException(
                "A serialized seat must belong to an existing billing account.");
        }

        var row = await Create(
                dbContext,
                dbContext.Seats
                    .AsNoTracking()
                    .Where(seat => seat.Id == seatId
                        && seat.BillingAccountId == billingAccountId))
            .SingleAsync(cancellationToken);
        var fundedSeatIds = await CommercialSeatFundingQuery.ResolveFundedSeatIdsAsync(
            dbContext,
            [row],
            cancellationToken);
        return SeatEntitlement.Resolve(
            row.ToSource(
                row.SubscriptionStatus is null || fundedSeatIds.Contains(row.SeatId)),
            observedAt);
    }

    public static IQueryable<SeatEntitlementRow> ForUser(
        LicensingDbContext dbContext,
        Guid userId)
    {
        return Create(
            dbContext,
            dbContext.Seats
                .AsNoTracking()
                .Where(seat => seat.AssignedUserId == userId)
                .OrderBy(seat => seat.CreatedAt)
                .ThenBy(seat => seat.Id));
    }

    private static IQueryable<SeatEntitlementRow> Create(
        LicensingDbContext dbContext,
        IQueryable<Seat> seats)
    {
        return from seat in seats
               join trial in dbContext.Trials.AsNoTracking()
                   on seat.BillingAccountId equals trial.BillingAccountId into matchingTrials
               from trial in matchingTrials.DefaultIfEmpty()
               join subscription in dbContext.CommercialSubscriptions.AsNoTracking()
                   on seat.BillingAccountId equals subscription.BillingAccountId
                   into matchingSubscriptions
               from subscription in matchingSubscriptions.DefaultIfEmpty()
               select new SeatEntitlementRow(
                   seat.Id,
                   seat.BillingAccountId,
                   seat.ProductAccessEnabled,
                   trial == null ? null : trial.StartedAt,
                   trial == null ? null : trial.EndsAt,
                   trial == null ? null : trial.TerminatedAt,
                   subscription == null ? null : subscription.Status,
                   subscription == null ? null : subscription.CurrentPeriodStartedAt,
                   subscription == null ? null : subscription.CurrentPeriodEndsAt,
                   subscription != null && subscription.CancelAtPeriodEnd,
                   subscription == null ? null : subscription.SeatQuantity);
    }
}
