using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Domain.Billing;

public enum BillingWebhookEventKind
{
    CheckoutCompleted,
    SubscriptionChanged,
    InvoicePaid,
    PaymentFailed,
    Unsupported,
}

public sealed class BillingWebhookEvent
{
    public const int MaximumExternalIdentifierLength = 255;

    public const int MaximumEventTypeLength = 128;

    private BillingWebhookEvent()
    {
    }

    private BillingWebhookEvent(
        Guid id,
        string externalEventId,
        string eventType,
        BillingWebhookEventKind kind,
        DateTimeOffset occurredAt,
        DateTimeOffset receivedAt)
    {
        Id = id;
        ExternalEventId = externalEventId;
        EventType = eventType;
        Kind = kind;
        OccurredAt = occurredAt;
        ReceivedAt = receivedAt;
    }

    // Upgrade marker: broker publication itself is owned by Rebus.
    internal bool NativeOutboxEnqueued { get; set; }

    public Guid Id { get; private set; }

    public string ExternalEventId { get; private set; } = string.Empty;

    public string EventType { get; private set; } = string.Empty;

    public BillingWebhookEventKind Kind { get; private set; }

    public DateTimeOffset OccurredAt { get; private set; }

    public DateTimeOffset ReceivedAt { get; private set; }

    public DateTimeOffset? ProcessedAt { get; private set; }

    public Guid? ProcessingLeaseId { get; private set; }

    public DateTimeOffset? ProcessingLeaseExpiresAt { get; private set; }

    public int ProcessingAttemptCount { get; private set; }

    public uint Version { get; private set; }

    public static BillingWebhookEvent Receive(
        string externalEventId,
        string eventType,
        BillingWebhookEventKind kind,
        DateTimeOffset occurredAt,
        DateTimeOffset receivedAt)
    {
        if (!Enum.IsDefined(kind))
        {
            throw new ArgumentOutOfRangeException(nameof(kind));
        }

        return new(
            Guid.CreateVersion7(),
            RequiredText.Normalize(
                externalEventId,
                nameof(externalEventId),
                MaximumExternalIdentifierLength,
                "external webhook event identifier"),
            RequiredText.Normalize(
                eventType,
                nameof(eventType),
                MaximumEventTypeLength,
                "webhook event type"),
            kind,
            occurredAt.ToUniversalTime(),
            receivedAt.ToUniversalTime());
    }

    public bool TryMarkProcessed(DateTimeOffset processedAt)
    {
        if (ProcessedAt is not null || ProcessingLeaseId is not null)
        {
            return false;
        }

        var utcProcessedAt = ValidateProcessingTime(processedAt);

        ProcessedAt = utcProcessedAt;
        return true;
    }

    public bool TryAcquireProcessingLease(
        Guid leaseId,
        DateTimeOffset acquiredAt,
        DateTimeOffset expiresAt)
    {

        var utcAcquiredAt = acquiredAt.ToUniversalTime();
        if (utcAcquiredAt < ReceivedAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(acquiredAt),
                "A webhook event cannot be leased before it was received.");
        }

        var utcExpiresAt = expiresAt.ToUniversalTime();
        if (utcExpiresAt <= utcAcquiredAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(expiresAt),
                "A processing lease must expire after it is acquired.");
        }

        if (ProcessedAt is not null
            || ProcessingLeaseExpiresAt is not null
                && ProcessingLeaseExpiresAt > utcAcquiredAt)
        {
            return false;
        }

        ProcessingLeaseId = leaseId;
        ProcessingLeaseExpiresAt = utcExpiresAt;
        ProcessingAttemptCount = checked(ProcessingAttemptCount + 1);
        return true;
    }

    public bool TryReleaseProcessingLease(Guid leaseId)
    {
        if (ProcessingLeaseId != leaseId || ProcessedAt is not null)
        {
            return false;
        }

        ProcessingLeaseId = null;
        ProcessingLeaseExpiresAt = null;
        return true;
    }

    public bool TryMarkProcessed(Guid leaseId, DateTimeOffset processedAt)
    {
        if (ProcessingLeaseId != leaseId || ProcessedAt is not null)
        {
            return false;
        }

        ProcessedAt = ValidateProcessingTime(processedAt);
        ProcessingLeaseId = null;
        ProcessingLeaseExpiresAt = null;
        return true;
    }

    private DateTimeOffset ValidateProcessingTime(DateTimeOffset processedAt)
    {
        var utcProcessedAt = processedAt.ToUniversalTime();
        if (utcProcessedAt < ReceivedAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(processedAt),
                "A webhook event cannot be processed before it was received.");
        }

        return utcProcessedAt;
    }
}
