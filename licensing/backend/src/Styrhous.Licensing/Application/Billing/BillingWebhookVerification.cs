using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Application.Billing;

public enum BillingWebhookVerificationStatus
{
    Verified,
    InvalidSignature,
    InvalidPayload,
}

public sealed record VerifiedBillingWebhookEvent(
    string ExternalEventId,
    string EventType,
    BillingWebhookEventKind Kind,
    DateTimeOffset OccurredAt);

public sealed record BillingWebhookVerificationResult(
    BillingWebhookVerificationStatus Status,
    VerifiedBillingWebhookEvent? Event)
{
    public static BillingWebhookVerificationResult Verified(
        VerifiedBillingWebhookEvent webhookEvent)
    {
        return new(BillingWebhookVerificationStatus.Verified, webhookEvent);
    }

    public static BillingWebhookVerificationResult InvalidSignature()
    {
        return new(BillingWebhookVerificationStatus.InvalidSignature, Event: null);
    }

    public static BillingWebhookVerificationResult InvalidPayload()
    {
        return new(BillingWebhookVerificationStatus.InvalidPayload, Event: null);
    }
}

public interface IBillingWebhookVerifier
{
    BillingWebhookVerificationResult Verify(string payload, string signature);
}
