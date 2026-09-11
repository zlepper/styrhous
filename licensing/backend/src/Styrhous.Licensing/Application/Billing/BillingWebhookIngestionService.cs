using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public enum BillingWebhookIngestionStatus
{
    Received,
    Duplicate,
    Ignored,
    InvalidSignature,
    InvalidPayload,
}

public sealed record BillingWebhookIngestionResult(
    BillingWebhookIngestionStatus Status,
    Guid? InboxEventId,
    BackgroundWorkReference? BackgroundWork);

public sealed class BillingWebhookIngestionService(
    IBillingWebhookVerifier verifier,
    PostgresBillingWebhookInboxStore store,
    TimeProvider timeProvider)
{

    public async Task<BillingWebhookIngestionResult> IngestAsync(
        string payload,
        string signature,
        CancellationToken cancellationToken = default)
    {
        var verification = verifier.Verify(payload, signature);
        if (verification.Status == BillingWebhookVerificationStatus.InvalidSignature)
        {
            return new BillingWebhookIngestionResult(
                BillingWebhookIngestionStatus.InvalidSignature,
                InboxEventId: null,
                BackgroundWork: null);
        }

        if (verification.Status == BillingWebhookVerificationStatus.InvalidPayload)
        {
            return new BillingWebhookIngestionResult(
                BillingWebhookIngestionStatus.InvalidPayload,
                InboxEventId: null,
                BackgroundWork: null);
        }

        var webhookEvent = verification.Event
            ?? throw new InvalidOperationException(
                "A verified webhook result must contain its event metadata.");
        var isSupported = webhookEvent.Kind != BillingWebhookEventKind.Unsupported;
        var receivedAt = timeProvider.GetUtcNow();
        var stored = await store.ReceiveAsync(
            webhookEvent,
            receivedAt,
            isSupported
                ? BillingWebhookInboxDisposition.PendingProcessing
                : BillingWebhookInboxDisposition.Ignored,
            cancellationToken);
        if (!isSupported)
        {
            return new BillingWebhookIngestionResult(
                BillingWebhookIngestionStatus.Ignored,
                stored.InboxEventId,
                BackgroundWork: null);
        }

        return stored.Status == BillingWebhookInboxStoreStatus.Received
            ? new BillingWebhookIngestionResult(
                BillingWebhookIngestionStatus.Received,
                stored.InboxEventId,
                BackgroundWorkReference.BillingWebhook(
                    stored.InboxEventId,
                    receivedAt))
            : new BillingWebhookIngestionResult(
                BillingWebhookIngestionStatus.Duplicate,
                stored.InboxEventId,
                BackgroundWork: null);
    }
}
