using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Domain.Trials;

public sealed class Trial
{
    public const int TransferredOrganizationSeatCapacity = 5;

    public static readonly TimeSpan Duration = TimeSpan.FromDays(30);

    private Trial()
    {
    }

    private Trial(
        Guid id,
        Guid originatingUserId,
        Guid billingAccountId,
        DateTimeOffset startedAt,
        DateTimeOffset endsAt)
    {
        Id = id;
        OriginatingUserId = originatingUserId;
        BillingAccountId = billingAccountId;
        StartedAt = startedAt;
        EndsAt = endsAt;
    }

    public Guid Id { get; private set; }

    public Guid OriginatingUserId { get; private set; }

    public Guid BillingAccountId { get; private set; }

    public DateTimeOffset StartedAt { get; private set; }

    public DateTimeOffset EndsAt { get; private set; }

    public DateTimeOffset? TransferredAt { get; private set; }

    public DateTimeOffset? TerminatedAt { get; private set; }

    public DateTimeOffset EffectiveEndsAt => TerminatedAt ?? EndsAt;

    public static Trial Start(
        Guid originatingUserId,
        Guid billingAccountId,
        DateTimeOffset startedAt)
    {

        var utcStartedAt = startedAt.ToUniversalTime();
        return new Trial(
            Uuid7.Create(),
            originatingUserId,
            billingAccountId,
            utcStartedAt,
            utcStartedAt.Add(Duration));
    }

    internal bool TryTransferToFirstOrganization(
        Guid billingAccountId,
        DateTimeOffset observedAt)
    {

        var utcObservedAt = observedAt.ToUniversalTime();
        if (TransferredAt is not null
            || TerminatedAt is not null
            || utcObservedAt < StartedAt
            || utcObservedAt >= EndsAt)
        {
            return false;
        }

        BillingAccountId = billingAccountId;
        TransferredAt = utcObservedAt;
        return true;
    }

    public bool TryTerminate(DateTimeOffset terminatedAt)
    {
        var utcTerminatedAt = terminatedAt.ToUniversalTime();
        if (utcTerminatedAt < StartedAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(terminatedAt),
                "A trial cannot terminate before it started.");
        }

        if (TerminatedAt is not null || utcTerminatedAt >= EndsAt)
        {
            return false;
        }

        TerminatedAt = utcTerminatedAt;
        return true;
    }
}
