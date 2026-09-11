using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Trials;

namespace Styrhous.Licensing.Persistence;

internal static class PostgresOrganizationSeatCapacity
{
    private sealed record CommercialCapacityProjection(
        CommercialSubscriptionStatus Status,
        int SeatQuantity,
        bool CancelsAtPeriodEnd,
        DateTimeOffset PeriodStartedAt,
        DateTimeOffset PeriodEndsAt);

    public static async Task<int?> SerializeBillingAccountAndResolveActiveAsync(
        LicensingDbContext dbContext,
        Guid billingAccountId,
        DateTimeOffset observedAt,
        CancellationToken cancellationToken)
    {
        var utcObservedAt = observedAt.ToUniversalTime();
        if (!await EfTransactionSerialization.TryClaimBillingAccountAsync(
                dbContext,
                billingAccountId,
                cancellationToken))
        {
            return null;
        }

        var subscription = await dbContext.CommercialSubscriptions
            .Where(subscription => subscription.BillingAccountId == billingAccountId)
            .Select(subscription => new CommercialCapacityProjection(
                subscription.Status,
                subscription.SeatQuantity,
                subscription.CancelAtPeriodEnd,
                subscription.CurrentPeriodStartedAt,
                subscription.CurrentPeriodEndsAt))
            .SingleOrDefaultAsync(cancellationToken);
        if (subscription is not null
            && CommercialEntitlement.Resolve(
                    subscription.Status,
                    subscription.CancelsAtPeriodEnd,
                    subscription.PeriodStartedAt,
                    subscription.PeriodEndsAt,
                    utcObservedAt)
                .IsEligible)
        {
            var pendingDecrease = await dbContext.BillingOperations
                .Where(operation => operation.BillingAccountId == billingAccountId
                    && operation.Kind == BillingOperationKind.SeatQuantityChange
                    && operation.Status == BillingOperationStatus.Pending
                    && operation.PreviousSeatQuantity > operation.SeatQuantity)
                .Select(operation => (int?)operation.SeatQuantity)
                .SingleOrDefaultAsync(cancellationToken);
            return pendingDecrease is null
                ? subscription.SeatQuantity
                : Math.Min(subscription.SeatQuantity, pendingDecrease.Value);
        }

        var hasActiveTransferredTrial = await dbContext.Trials.AnyAsync(
            trial => trial.BillingAccountId == billingAccountId
                && trial.TransferredAt != null
                && trial.StartedAt <= utcObservedAt
                && utcObservedAt < trial.EndsAt
                && (trial.TerminatedAt == null || utcObservedAt < trial.TerminatedAt),
            cancellationToken);
        return hasActiveTransferredTrial
            ? Trial.TransferredOrganizationSeatCapacity
            : null;
    }

}
