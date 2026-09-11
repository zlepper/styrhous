using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Persistence;

internal static class PostgresCommercialSubscriptionProjection
{
    public static async Task<CommercialSubscriptionProjectionResult> ApplyClaimedAsync(
        LicensingDbContext dbContext,
        Guid billingAccountId,
        CommercialSubscriptionProjection projection,
        long providerReadRevision,
        CommercialSubscriptionSnapshotKind providerSnapshotKind,
        CancellationToken cancellationToken,
        Guid? checkoutOperationId = null)
    {
        if (dbContext.Database.CurrentTransaction is null)
        {
            throw new InvalidOperationException(
                "Commercial subscription projection requires an active EF transaction.");
        }

        ArgumentOutOfRangeException.ThrowIfNegative(providerReadRevision);
        if (!Enum.IsDefined(providerSnapshotKind))
        {
            throw new ArgumentOutOfRangeException(nameof(providerSnapshotKind));
        }

        var subscription = dbContext.CommercialSubscriptions.Local.SingleOrDefault(
            candidate => candidate.BillingAccountId == billingAccountId);
        if (subscription is not null)
        {
            await dbContext.Entry(subscription).ReloadAsync(cancellationToken);
            if (dbContext.Entry(subscription).State == EntityState.Detached)
            {
                subscription = null;
            }
        }

        subscription ??= await dbContext.CommercialSubscriptions.SingleOrDefaultAsync(
            candidate => candidate.BillingAccountId == billingAccountId,
            cancellationToken);
        var checkout = checkoutOperationId is Guid operationId
            ? await dbContext.BillingOperations.SingleOrDefaultAsync(operation => operation.Id == operationId,
                cancellationToken) : null;
        var authorizedCheckout = checkout is not null
            && checkout.Kind == BillingOperationKind.InitialCheckout
            && checkout.BillingAccountId == billingAccountId
            && checkout.Status is BillingOperationStatus.Pending or BillingOperationStatus.ProviderSessionCreated
            && (checkout.ExternalSubscriptionId is null || checkout.ExternalSubscriptionId == projection.ExternalSubscriptionId)
            && projection.ProjectedAt >= checkout.CreatedAt;
        CommercialSubscriptionProjectionStatus status;
        if (subscription is null)
        {
            subscription = CommercialSubscription.Create(
                billingAccountId,
                projection,
                providerReadRevision,
                providerSnapshotKind);
            dbContext.CommercialSubscriptions.Add(subscription);
            status = CommercialSubscriptionProjectionStatus.Created;
        }
        else if (subscription.ExternalSubscriptionId != projection.ExternalSubscriptionId)
        {
            status = authorizedCheckout && checkout!.PreviousSubscription is { } predecessor
                && subscription.TryReplaceProjection(projection, predecessor, providerReadRevision, providerSnapshotKind)
                    ? CommercialSubscriptionProjectionStatus.Updated : CommercialSubscriptionProjectionStatus.Ignored;
        }
        else if (providerReadRevision > 0
            ? subscription.TryApplyProjection(
                projection,
                providerReadRevision,
                providerSnapshotKind)
            : subscription.TryApplyProjection(projection))
        {
            status = CommercialSubscriptionProjectionStatus.Updated;
        }
        else
        {
            status = providerReadRevision > 0
                && subscription.ConflictsWithEqualTimeMutationResponse(
                    projection,
                    providerSnapshotKind)
                ? CommercialSubscriptionProjectionStatus.CausalConflict
                : CommercialSubscriptionProjectionStatus.Ignored;
        }

        if (authorizedCheckout && subscription.ExternalSubscriptionId == projection.ExternalSubscriptionId)
        {
            checkout!.CompleteProjectedCheckout(projection.ExternalSubscriptionId, projection.ProjectedAt);
        }

        await TerminateInternalTrialForEligibleSubscriptionAsync(
            dbContext,
            subscription,
            cancellationToken);
        return new CommercialSubscriptionProjectionResult(status, subscription.Id);
    }

    private static async Task TerminateInternalTrialForEligibleSubscriptionAsync(
        LicensingDbContext dbContext,
        CommercialSubscription subscription,
        CancellationToken cancellationToken)
    {
        if (!CommercialEntitlement.Resolve(
                subscription.Status,
                subscription.CancelAtPeriodEnd,
                subscription.CurrentPeriodStartedAt,
                subscription.CurrentPeriodEndsAt,
                subscription.ProjectedAt)
            .IsEligible)
        {
            return;
        }

        var trial = dbContext.Trials.Local.SingleOrDefault(
            candidate => candidate.BillingAccountId == subscription.BillingAccountId);
        trial ??= await dbContext.Trials.SingleOrDefaultAsync(
            candidate => candidate.BillingAccountId == subscription.BillingAccountId,
            cancellationToken);
        trial?.TryTerminate(subscription.ProjectedAt);
    }
}
