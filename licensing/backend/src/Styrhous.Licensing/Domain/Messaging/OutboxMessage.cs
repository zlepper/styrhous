using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Domain.Messaging;

public enum OutboxDiscardReason
{
    Superseded,
    InvitationCancelled,
    InvitationAccepted,
    UndeliverableProtectedPayload,
}

public sealed class OutboxMessage
{
    public const int MaximumMessageTypeLength = 128;

    public const int MaximumProtectedPayloadLength = 65_536;

    private OutboxMessage()
    {
    }

    private OutboxMessage(
        Guid id,
        Guid correlationId,
        Guid subjectId,
        string messageType,
        string protectedPayload,
        DateTimeOffset occurredAt,
        DateTimeOffset? notAfter)
    {
        Id = id;
        CorrelationId = correlationId;
        SubjectId = subjectId;
        MessageType = messageType;
        ProtectedPayload = protectedPayload;
        OccurredAt = occurredAt;
        NotAfter = notAfter;
    }

    // Upgrade marker: broker publication itself is owned by Rebus.
    internal bool NativeOutboxEnqueued { get; set; }

    public Guid Id { get; private set; }

    public Guid CorrelationId { get; private set; }

    public Guid SubjectId { get; private set; }

    public string MessageType { get; private set; } = string.Empty;

    public string ProtectedPayload { get; private set; } = string.Empty;

    public DateTimeOffset OccurredAt { get; private set; }

    public DateTimeOffset? NotAfter { get; private set; }

    public DateTimeOffset? DeliveredAt { get; private set; }

    public Guid? ProcessingLeaseId { get; private set; }

    public DateTimeOffset? ProcessingLeaseExpiresAt { get; private set; }

    public int ProcessingAttemptCount { get; private set; }

    public DateTimeOffset? DiscardedAt { get; private set; }

    public OutboxDiscardReason? DiscardReason { get; private set; }

    public uint Version { get; private set; }

    public static OutboxMessage Enqueue(
        Guid correlationId,
        Guid subjectId,
        string messageType,
        string protectedPayload,
        DateTimeOffset occurredAt,
        DateTimeOffset? notAfter = null)
    {

        var utcOccurredAt = occurredAt.ToUniversalTime();
        var utcNotAfter = notAfter?.ToUniversalTime();
        if (utcNotAfter <= utcOccurredAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(notAfter),
                "An outbox message expiry must be after it occurred.");
        }

        return new OutboxMessage(
            Uuid7.Create(),
            correlationId,
            subjectId,
            RequiredText.Normalize(
                messageType,
                nameof(messageType),
                MaximumMessageTypeLength,
                "outbox message type"),
            RequiredText.Normalize(
                protectedPayload,
                nameof(protectedPayload),
                MaximumProtectedPayloadLength,
                "protected outbox payload"),
            utcOccurredAt,
            utcNotAfter);
    }

    public bool TryAcquireProcessingLease(
        Guid leaseId,
        DateTimeOffset acquiredAt,
        DateTimeOffset expiresAt)
    {

        var utcAcquiredAt = acquiredAt.ToUniversalTime();
        if (utcAcquiredAt < OccurredAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(acquiredAt),
                "An outbox message cannot be leased before it occurred.");
        }

        var utcExpiresAt = expiresAt.ToUniversalTime();
        if (utcExpiresAt <= utcAcquiredAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(expiresAt),
                "A processing lease must expire after it is acquired.");
        }

        if (DeliveredAt is not null
            || DiscardedAt is not null
            || NotAfter is not null && NotAfter <= utcAcquiredAt
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

    public bool TryMarkDelivered(Guid leaseId, DateTimeOffset deliveredAt)
    {

        var utcDeliveredAt = deliveredAt.ToUniversalTime();
        if (utcDeliveredAt < OccurredAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(deliveredAt),
                "An outbox message cannot be delivered before it occurred.");
        }

        if (ProcessingLeaseId != leaseId
            || DeliveredAt is not null
            || DiscardedAt is not null
            || NotAfter is not null && NotAfter <= utcDeliveredAt)
        {
            return false;
        }

        DeliveredAt = utcDeliveredAt;
        ProcessingLeaseId = null;
        ProcessingLeaseExpiresAt = null;
        return true;
    }

    public bool TryReleaseProcessingLease(Guid leaseId)
    {
        if (ProcessingLeaseId != leaseId
            || DeliveredAt is not null
            || DiscardedAt is not null)
        {
            return false;
        }

        ProcessingLeaseId = null;
        ProcessingLeaseExpiresAt = null;
        return true;
    }

    public bool TryDiscard(
        OutboxDiscardReason reason,
        DateTimeOffset discardedAt)
    {
        if (!Enum.IsDefined(reason))
        {
            throw new ArgumentOutOfRangeException(nameof(reason));
        }

        var utcDiscardedAt = discardedAt.ToUniversalTime();
        if (utcDiscardedAt < OccurredAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(discardedAt),
                "An outbox message cannot be discarded before it occurred.");
        }

        if (DeliveredAt is not null || DiscardedAt is not null)
        {
            return false;
        }

        DiscardedAt = utcDiscardedAt;
        DiscardReason = reason;
        ProcessingLeaseId = null;
        ProcessingLeaseExpiresAt = null;
        return true;
    }
}
