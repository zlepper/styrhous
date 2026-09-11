using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Domain.Billing;

public sealed record CommercialSubscriptionProjection
{
    public CommercialSubscriptionProjection(
        string externalCustomerId,
        string externalSubscriptionId,
        string externalPriceId,
        CommercialSubscriptionStatus status,
        int seatQuantity,
        bool cancelAtPeriodEnd,
        DateTimeOffset currentPeriodStartedAt,
        DateTimeOffset currentPeriodEndsAt,
        DateTimeOffset projectedAt)
    {
        if (!Enum.IsDefined(status))
        {
            throw new ArgumentOutOfRangeException(nameof(status));
        }

        if (seatQuantity <= 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(seatQuantity),
                "A commercial subscription must fund at least one seat.");
        }

        var utcPeriodStartedAt = currentPeriodStartedAt.ToUniversalTime();
        var utcPeriodEndsAt = currentPeriodEndsAt.ToUniversalTime();
        if (utcPeriodEndsAt <= utcPeriodStartedAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(currentPeriodEndsAt),
                "A commercial subscription period must end after it starts.");
        }

        if (projectedAt == default)
        {
            throw new ArgumentOutOfRangeException(
                nameof(projectedAt),
                "A commercial subscription requires its provider observation time.");
        }

        ExternalCustomerId = RequiredText.Normalize(
            externalCustomerId,
            nameof(externalCustomerId),
            CommercialSubscription.MaximumExternalIdentifierLength,
            "external customer identifier");
        ExternalSubscriptionId = RequiredText.Normalize(
            externalSubscriptionId,
            nameof(externalSubscriptionId),
            CommercialSubscription.MaximumExternalIdentifierLength,
            "external subscription identifier");
        ExternalPriceId = RequiredText.Normalize(
            externalPriceId,
            nameof(externalPriceId),
            CommercialSubscription.MaximumExternalIdentifierLength,
            "external price identifier");
        Status = status;
        SeatQuantity = seatQuantity;
        CancelAtPeriodEnd = cancelAtPeriodEnd;
        CurrentPeriodStartedAt = utcPeriodStartedAt;
        CurrentPeriodEndsAt = utcPeriodEndsAt;
        ProjectedAt = projectedAt.ToUniversalTime();
    }

    public string ExternalCustomerId { get; }

    public string ExternalSubscriptionId { get; }

    public string ExternalPriceId { get; }

    public CommercialSubscriptionStatus Status { get; }

    public int SeatQuantity { get; }

    public bool CancelAtPeriodEnd { get; }

    public DateTimeOffset CurrentPeriodStartedAt { get; }

    public DateTimeOffset CurrentPeriodEndsAt { get; }

    public DateTimeOffset ProjectedAt { get; }
}
