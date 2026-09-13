using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public enum BillingWebhookProcessingClaimStatus
{
    Acquired,
    NotFound,
    AlreadyProcessed,
    Busy,
}

public sealed record BillingWebhookProcessingClaim(
    Guid InboxEventId,
    Guid LeaseId,
    string ExternalEventId,
    BillingWebhookEventKind Kind);

public sealed record BillingWebhookProcessingClaimResult(
    BillingWebhookProcessingClaimStatus Status,
    BillingWebhookProcessingClaim? Claim);

public enum BillingWebhookProcessingStatus
{
    Processed,
    Ignored,
    NotFound,
    AlreadyProcessed,
    Busy,
}

public sealed record BillingWebhookProcessingResult(BillingWebhookProcessingStatus Status);

public sealed record AuthoritativeCommercialSubscription(
    Guid BillingAccountId,
    CommercialSubscriptionProjection Projection,
    long ProviderReadRevision = 0,
    CommercialSubscriptionSnapshotKind ProviderSnapshotKind =
        CommercialSubscriptionSnapshotKind.Observation,
    Guid? BillingOperationId = null);
