using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Application.Billing;

public sealed class BillingWebhookProcessingService(
    PostgresBillingWebhookProcessingStore processingStore,
    ICommercialSubscriptionProvider subscriptionProvider,
    CommercialSubscriptionProjectionService projectionService,
    TimeProvider timeProvider)
{
    private static readonly TimeSpan ProcessingLeaseDuration = TimeSpan.FromMinutes(5);

    private static readonly TimeSpan LeaseReleaseTimeout = TimeSpan.FromSeconds(5);

    public async Task<BillingWebhookProcessingResult> ProcessAsync(
        Guid inboxEventId,
        CancellationToken cancellationToken = default)
    {

        var acquiredAt = timeProvider.GetUtcNow();
        var leaseId = Uuid7.Create();
        var claimResult = await processingStore.TryAcquireAsync(
            inboxEventId,
            leaseId,
            acquiredAt,
            acquiredAt.Add(ProcessingLeaseDuration),
            cancellationToken);
        if (claimResult.Status != BillingWebhookProcessingClaimStatus.Acquired)
        {
            return new BillingWebhookProcessingResult(MapClaimStatus(claimResult.Status));
        }

        var claim = claimResult.Claim
            ?? throw new InvalidOperationException(
                "An acquired billing webhook processing claim is required.");
        try
        {
            var subscription = await subscriptionProvider.ResolveEventAsync(
                claim.ExternalEventId,
                claim.Kind,
                cancellationToken);
            var status = BillingWebhookProcessingStatus.Ignored;
            if (subscription is not null)
            {
                var projectionResult = await projectionService.ApplyAsync(
                    subscription,
                    cancellationToken);
                if (projectionResult.Status
                    == CommercialSubscriptionProjectionStatus.BillingAccountNotFound)
                {
                    throw new InvalidOperationException(
                        "The billing provider referenced an unknown billing account.");
                }

                if (projectionResult.Status
                    == CommercialSubscriptionProjectionStatus.CausalConflict)
                {
                    throw new InvalidOperationException(
                        "The billing provider snapshot overlaps a successful mutation; "
                            + "the webhook must be resolved again.");
                }

                status = BillingWebhookProcessingStatus.Processed;
            }

            if (!await processingStore.CompleteAsync(
                    claim.InboxEventId,
                    claim.LeaseId,
                    timeProvider.GetUtcNow(),
                    cancellationToken))
            {
                throw new InvalidOperationException(
                    "The billing webhook processing lease was lost before completion.");
            }

            return new BillingWebhookProcessingResult(status);
        }
        catch (Exception processingException)
        {
            using var releaseTimeout = new CancellationTokenSource(LeaseReleaseTimeout);
            try
            {
                await processingStore.ReleaseAsync(
                    claim.InboxEventId,
                    claim.LeaseId,
                    releaseTimeout.Token);
            }
            catch (Exception releaseException)
            {
                throw new AggregateException(
                    "Billing webhook processing and lease release both failed.",
                    processingException,
                    releaseException);
            }

            throw;
        }
    }

    private static BillingWebhookProcessingStatus MapClaimStatus(
        BillingWebhookProcessingClaimStatus status)
    {
        return status switch
        {
            BillingWebhookProcessingClaimStatus.NotFound =>
                BillingWebhookProcessingStatus.NotFound,
            BillingWebhookProcessingClaimStatus.AlreadyProcessed =>
                BillingWebhookProcessingStatus.AlreadyProcessed,
            BillingWebhookProcessingClaimStatus.Busy => BillingWebhookProcessingStatus.Busy,
            _ => throw new ArgumentOutOfRangeException(nameof(status)),
        };
    }
}
