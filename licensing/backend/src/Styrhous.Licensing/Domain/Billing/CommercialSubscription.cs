using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Domain.Billing;

public enum CommercialSubscriptionStatus
{
    Active,
    PastDue,
    Unpaid,
    Paused,
    Incomplete,
    IncompleteExpired,
    Trialing,
    Canceled,
}

public enum CommercialSubscriptionSnapshotKind
{
    Observation,
    MutationResponse,
}

public sealed class CommercialSubscription
{
    public const int MaximumExternalIdentifierLength = 255;

    private CommercialSubscription()
    {
    }

    private CommercialSubscription(
        Guid id,
        Guid billingAccountId,
        CommercialSubscriptionProjection projection,
        long providerReadRevision,
        CommercialSubscriptionSnapshotKind providerSnapshotKind)
    {
        Id = id;
        BillingAccountId = billingAccountId;
        Apply(projection, providerReadRevision, providerSnapshotKind);
    }

    public Guid Id { get; private set; }

    public Guid BillingAccountId { get; private set; }

    public string ExternalCustomerId { get; private set; } = string.Empty;

    public string ExternalSubscriptionId { get; private set; } = string.Empty;

    public string ExternalPriceId { get; private set; } = string.Empty;

    public CommercialSubscriptionStatus Status { get; private set; }

    public int SeatQuantity { get; private set; }

    public bool CancelAtPeriodEnd { get; private set; }

    public DateTimeOffset CurrentPeriodStartedAt { get; private set; }

    public DateTimeOffset CurrentPeriodEndsAt { get; private set; }

    public DateTimeOffset ProjectedAt { get; private set; }

    public long ProviderReadRevision { get; private set; }

    public CommercialSubscriptionSnapshotKind ProviderSnapshotKind { get; private set; }

    public uint Version { get; private set; }

    public CommercialSubscriptionProjection Snapshot()
    {
        return new(
        ExternalCustomerId, ExternalSubscriptionId, ExternalPriceId, Status,
        SeatQuantity, CancelAtPeriodEnd, CurrentPeriodStartedAt, CurrentPeriodEndsAt, ProjectedAt);
    }

    public bool TryReplaceProjection(CommercialSubscriptionProjection projection,
        CommercialSubscriptionProjection predecessor, long providerReadRevision,
        CommercialSubscriptionSnapshotKind providerSnapshotKind)
    {
        ArgumentNullException.ThrowIfNull(projection);
        ArgumentNullException.ThrowIfNull(predecessor);
        ArgumentOutOfRangeException.ThrowIfNegative(providerReadRevision);
        ValidateSnapshotKind(providerSnapshotKind);
        if (!BillingCheckoutEligibility.AllowsPurchase(Status)
            || predecessor.ExternalSubscriptionId != ExternalSubscriptionId
            || projection.ExternalCustomerId != ExternalCustomerId
            || projection.ExternalSubscriptionId == ExternalSubscriptionId)
        {
            return false;
        }

        Apply(projection, providerReadRevision, providerSnapshotKind);
        return true;
    }

    public static CommercialSubscription Create(
        Guid billingAccountId,
        CommercialSubscriptionProjection projection,
        long providerReadRevision = 0,
        CommercialSubscriptionSnapshotKind providerSnapshotKind =
            CommercialSubscriptionSnapshotKind.Observation)
    {

        ArgumentNullException.ThrowIfNull(projection);
        ArgumentOutOfRangeException.ThrowIfNegative(providerReadRevision);
        ValidateSnapshotKind(providerSnapshotKind);
        return new CommercialSubscription(
            Uuid7.Create(),
            billingAccountId,
            projection,
            providerReadRevision,
            providerSnapshotKind);
    }

    public bool TryApplyProjection(CommercialSubscriptionProjection projection)
    {
        ArgumentNullException.ThrowIfNull(projection);
        if (projection.ExternalSubscriptionId != ExternalSubscriptionId
            || ProviderReadRevision > 0 || projection.ProjectedAt <= ProjectedAt)
        {
            return false;
        }

        Apply(
            projection,
            ProviderReadRevision,
            CommercialSubscriptionSnapshotKind.Observation);
        return true;
    }

    public bool TryApplyProjection(
        CommercialSubscriptionProjection projection,
        long providerReadRevision,
        CommercialSubscriptionSnapshotKind providerSnapshotKind =
            CommercialSubscriptionSnapshotKind.Observation)
    {
        ArgumentNullException.ThrowIfNull(projection);
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(providerReadRevision);
        ValidateSnapshotKind(providerSnapshotKind);
        if (projection.ExternalSubscriptionId != ExternalSubscriptionId
            || projection.ProjectedAt < ProjectedAt
            || (projection.ProjectedAt == ProjectedAt
                && (providerSnapshotKind < ProviderSnapshotKind
                    || (providerSnapshotKind == ProviderSnapshotKind
                        && providerReadRevision <= ProviderReadRevision))))
        {
            return false;
        }

        Apply(projection, providerReadRevision, providerSnapshotKind);
        return true;
    }

    public bool MatchesProviderSnapshot(
        CommercialSubscriptionProjection projection,
        long providerReadRevision,
        CommercialSubscriptionSnapshotKind providerSnapshotKind)
    {
        ArgumentNullException.ThrowIfNull(projection);
        ArgumentOutOfRangeException.ThrowIfNegative(providerReadRevision);
        ValidateSnapshotKind(providerSnapshotKind);
        return ProviderReadRevision == providerReadRevision
            && ProviderSnapshotKind == providerSnapshotKind
            && MatchesProjection(projection);
    }

    public bool ConflictsWithEqualTimeMutationResponse(
        CommercialSubscriptionProjection projection,
        CommercialSubscriptionSnapshotKind providerSnapshotKind)
    {
        ArgumentNullException.ThrowIfNull(projection);
        ValidateSnapshotKind(providerSnapshotKind);
        return ProviderSnapshotKind == CommercialSubscriptionSnapshotKind.MutationResponse
            && providerSnapshotKind == CommercialSubscriptionSnapshotKind.Observation
            && ProjectedAt == projection.ProjectedAt
            && !MatchesProjection(projection);
    }

    private bool MatchesProjection(CommercialSubscriptionProjection projection)
    {
        return ExternalCustomerId == projection.ExternalCustomerId
            && ExternalSubscriptionId == projection.ExternalSubscriptionId
            && ExternalPriceId == projection.ExternalPriceId
            && Status == projection.Status
            && SeatQuantity == projection.SeatQuantity
            && CancelAtPeriodEnd == projection.CancelAtPeriodEnd
            && CurrentPeriodStartedAt == projection.CurrentPeriodStartedAt
            && CurrentPeriodEndsAt == projection.CurrentPeriodEndsAt
            && ProjectedAt == projection.ProjectedAt;
    }

    private void Apply(
        CommercialSubscriptionProjection projection,
        long providerReadRevision,
        CommercialSubscriptionSnapshotKind providerSnapshotKind)
    {
        ExternalCustomerId = projection.ExternalCustomerId;
        ExternalSubscriptionId = projection.ExternalSubscriptionId;
        ExternalPriceId = projection.ExternalPriceId;
        Status = projection.Status;
        SeatQuantity = projection.SeatQuantity;
        CancelAtPeriodEnd = projection.CancelAtPeriodEnd;
        CurrentPeriodStartedAt = projection.CurrentPeriodStartedAt;
        CurrentPeriodEndsAt = projection.CurrentPeriodEndsAt;
        ProjectedAt = projection.ProjectedAt;
        ProviderReadRevision = providerReadRevision;
        ProviderSnapshotKind = providerSnapshotKind;
    }

    private static void ValidateSnapshotKind(
        CommercialSubscriptionSnapshotKind providerSnapshotKind)
    {
        if (!Enum.IsDefined(providerSnapshotKind))
        {
            throw new ArgumentOutOfRangeException(nameof(providerSnapshotKind));
        }
    }

}
