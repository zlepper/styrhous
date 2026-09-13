using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Domain.Billing;

public enum BillingOperationKind
{
    InitialCheckout,
    SeatQuantityChange,
}

public enum BillingOperationStatus
{
    Pending,
    ProviderSessionCreated,
    Completed,
    Failed,
    Expired,
}

public enum BillingCadence
{
    Monthly,
    Annual,
}

public enum SeatQuantityChangeOutcome
{
    Applied,
    Superseded,
    ProviderRejected,
}

public sealed class BillingOperation
{
    public const int MaximumExternalIdentifierLength = 255;

    public static readonly TimeSpan CheckoutLifetime = TimeSpan.FromHours(1);

    private BillingOperation()
    {
    }

    private BillingOperation(
        Guid id,
        Guid billingAccountId,
        Guid actorUserId,
        BillingOperationKind kind,
        BillingCadence? cadence,
        int? previousSeatQuantity,
        int seatQuantity,
        DateTimeOffset createdAt,
        DateTimeOffset? expiresAt)
    {
        Id = id;
        BillingAccountId = billingAccountId;
        ActorUserId = actorUserId;
        Kind = kind;
        Cadence = cadence;
        PreviousSeatQuantity = previousSeatQuantity;
        SeatQuantity = seatQuantity;
        Status = BillingOperationStatus.Pending;
        CreatedAt = createdAt;
        ExpiresAt = expiresAt;
    }

    public Guid Id { get; private set; }

    public Guid BillingAccountId { get; private set; }

    public Guid ActorUserId { get; private set; }

    public BillingOperationKind Kind { get; private set; }

    public BillingCadence? Cadence { get; private set; }

    public int? PreviousSeatQuantity { get; private set; }

    public int SeatQuantity { get; private set; }

    public BillingOperationStatus Status { get; private set; }

    public DateTimeOffset CreatedAt { get; private set; }

    public DateTimeOffset? ExpiresAt { get; private set; }

    public DateTimeOffset? ClosedAt { get; private set; }

    public DateTimeOffset? ProviderSessionRecordedAt { get; private set; }

    public string? ExternalSessionId { get; private set; }

    // Immutable checkout input also retains the terminal subscription history.
    public CommercialSubscriptionProjection? PreviousSubscription { get; private set; }

    public string? ExternalSubscriptionId { get; private set; }

    public void RecordCheckoutSubscription(string externalSubscriptionId)
    {
        EnsureKind(BillingOperationKind.InitialCheckout);
        if (ExternalSubscriptionId is not null && ExternalSubscriptionId != externalSubscriptionId)
            throw new InvalidOperationException("The Checkout subscription cannot change.");
        ExternalSubscriptionId = RequiredText.Normalize(externalSubscriptionId,
            nameof(externalSubscriptionId), MaximumExternalIdentifierLength, "external subscription identifier");
    }

    public DateTimeOffset? ProviderMutationReplayStartedAt { get; private set; }

    public SeatQuantityChangeOutcome? SeatQuantityOutcome { get; private set; }

    public uint Version { get; private set; }

    public BillingCadence RequireCheckoutCadence()
    {
        EnsureKind(BillingOperationKind.InitialCheckout);
        return Cadence
            ?? throw new InvalidOperationException(
                "An initial Checkout operation must have a cadence.");
    }

    public DateTimeOffset RequireCheckoutExpiry()
    {
        EnsureKind(BillingOperationKind.InitialCheckout);
        return ExpiresAt
            ?? throw new InvalidOperationException(
                "An initial Checkout operation must have an expiry.");
    }

    public int RequirePreviousSeatQuantity()
    {
        EnsureKind(BillingOperationKind.SeatQuantityChange);
        return PreviousSeatQuantity
            ?? throw new InvalidOperationException(
                "A seat-quantity change must have a previous quantity.");
    }

    public SeatQuantityChangeOutcome RequireSeatQuantityOutcome()
    {
        EnsureKind(BillingOperationKind.SeatQuantityChange);
        return SeatQuantityOutcome
            ?? throw new InvalidOperationException(
                "A terminal seat-quantity change must have an outcome.");
    }

    public static BillingOperation StartInitialCheckout(
        Guid billingAccountId,
        Guid actorUserId,
        BillingCadence cadence,
        int seatQuantity,
        DateTimeOffset createdAt,
        CommercialSubscriptionProjection? previousSubscription = null)
    {

        if (!Enum.IsDefined(cadence))
        {
            throw new ArgumentOutOfRangeException(nameof(cadence));
        }

        if (seatQuantity <= 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(seatQuantity),
                "A billing operation must contain at least one seat.");
        }

        if (!BillingCheckoutEligibility.AllowsPurchase(previousSubscription?.Status))
            throw new ArgumentException("Only terminal subscriptions can be replaced.", nameof(previousSubscription));

        return new BillingOperation(
            Guid.CreateVersion7(),
            billingAccountId,
            actorUserId,
            BillingOperationKind.InitialCheckout,
            cadence,
            previousSeatQuantity: null,
            seatQuantity,
            createdAt.ToUniversalTime(),
            createdAt.ToUniversalTime().Add(CheckoutLifetime))
        {
            PreviousSubscription = previousSubscription,
        };
    }

    public static BillingOperation StartSeatQuantityChange(
        Guid billingAccountId,
        Guid actorUserId,
        int previousSeatQuantity,
        int seatQuantity,
        DateTimeOffset createdAt)
    {

        if (previousSeatQuantity <= 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(previousSeatQuantity),
                "The previous subscription quantity must contain at least one seat.");
        }

        if (seatQuantity <= 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(seatQuantity),
                "A billing operation must contain at least one seat.");
        }

        if (seatQuantity == previousSeatQuantity)
        {
            throw new ArgumentException(
                "A seat-quantity change must change the subscription quantity.",
                nameof(seatQuantity));
        }

        return new BillingOperation(
            Guid.CreateVersion7(),
            billingAccountId,
            actorUserId,
            BillingOperationKind.SeatQuantityChange,
            cadence: null,
            previousSeatQuantity,
            seatQuantity,
            createdAt.ToUniversalTime(),
            expiresAt: null);
    }

    public bool TryRecordProviderSession(
        string externalSessionId,
        DateTimeOffset recordedAt)
    {
        EnsureKind(BillingOperationKind.InitialCheckout);
        var normalizedSessionId = RequiredText.Normalize(
            externalSessionId,
            nameof(externalSessionId),
            MaximumExternalIdentifierLength,
            "external Checkout session identifier");
        if (ExternalSessionId is not null)
        {
            if (!string.Equals(
                    ExternalSessionId,
                    normalizedSessionId,
                    StringComparison.Ordinal))
            {
                throw new InvalidOperationException(
                    "A billing operation cannot record a different provider session.");
            }

            return false;
        }

        if (Status == BillingOperationStatus.Completed && ExternalSubscriptionId is not null)
        {
            return false;
        }

        if (Status != BillingOperationStatus.Pending)
        {
            throw new InvalidOperationException(
                "Only a pending billing operation can record a provider session.");
        }

        var utcRecordedAt = recordedAt.ToUniversalTime();
        if (utcRecordedAt < CreatedAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(recordedAt),
                "A provider session cannot be recorded before its billing operation.");
        }

        ExternalSessionId = normalizedSessionId;
        ProviderSessionRecordedAt = utcRecordedAt;
        Status = BillingOperationStatus.ProviderSessionCreated;
        return true;
    }

    public bool TryFailProviderSessionCreation(DateTimeOffset failedAt)
    {
        EnsureKind(BillingOperationKind.InitialCheckout);
        if (Status == BillingOperationStatus.Failed)
        {
            return false;
        }

        if (Status != BillingOperationStatus.Pending)
        {
            return false;
        }

        var utcFailedAt = failedAt.ToUniversalTime();
        if (utcFailedAt < CreatedAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(failedAt),
                "A provider-session failure cannot predate its billing operation.");
        }

        Status = BillingOperationStatus.Failed;
        ClosedAt = utcFailedAt;
        return true;
    }

    public bool TryCompleteProviderSession(DateTimeOffset completedAt)
    {
        EnsureKind(BillingOperationKind.InitialCheckout);
        if (Status == BillingOperationStatus.Completed)
        {
            return false;
        }

        if (Status != BillingOperationStatus.ProviderSessionCreated)
        {
            throw new InvalidOperationException(
                "Only a recorded provider session can be completed.");
        }

        var utcCompletedAt = ValidateClosingTime(completedAt, "completion");
        var recordedAt = ProviderSessionRecordedAt
            ?? throw new InvalidOperationException(
                "A recorded provider session must have its recorded time.");
        if (utcCompletedAt < recordedAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(completedAt),
                "A provider session cannot complete before it was recorded.");
        }

        ClosedAt = utcCompletedAt;
        Status = BillingOperationStatus.Completed;
        return true;
    }

    public void CompleteProjectedCheckout(string externalSubscriptionId, DateTimeOffset observedAt)
    {
        EnsureKind(BillingOperationKind.InitialCheckout);
        if (Status is not (BillingOperationStatus.Pending or BillingOperationStatus.ProviderSessionCreated))
            throw new InvalidOperationException("Only a live Checkout can accept its subscription projection.");
        var completedAt = ValidateClosingTime(observedAt, "completion");
        RecordCheckoutSubscription(externalSubscriptionId);
        // A webhook can beat the API's session recording, or wait behind its lock.
        // The authoritative subscription itself proves completion in either order.
        ClosedAt = ProviderSessionRecordedAt is { } recordedAt && recordedAt > completedAt
            ? recordedAt : completedAt;
        Status = BillingOperationStatus.Completed;
    }

    public bool TryExpireProviderSession(
        string externalSessionId,
        DateTimeOffset expiredAt)
    {
        EnsureKind(BillingOperationKind.InitialCheckout);
        var normalizedSessionId = RequiredText.Normalize(
            externalSessionId,
            nameof(externalSessionId),
            MaximumExternalIdentifierLength,
            "external Checkout session identifier");
        if (ExternalSessionId is not null
            && !string.Equals(
                ExternalSessionId,
                normalizedSessionId,
                StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "A billing operation cannot expire a different provider session.");
        }

        if (Status == BillingOperationStatus.Expired)
        {
            return false;
        }

        if (Status != BillingOperationStatus.ProviderSessionCreated)
        {
            throw new InvalidOperationException(
                "Only a recorded provider session can be expired.");
        }

        var utcExpiredAt = expiredAt.ToUniversalTime();
        var recordedAt = ProviderSessionRecordedAt
            ?? throw new InvalidOperationException(
                "A recorded provider session must have its recorded time.");
        if (utcExpiredAt < recordedAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(expiredAt),
                "A provider session cannot expire before it was created.");
        }

        Status = BillingOperationStatus.Expired;
        ClosedAt = utcExpiredAt;
        return true;
    }

    public bool TryCompleteSeatQuantityChange(DateTimeOffset completedAt)
    {
        EnsureKind(BillingOperationKind.SeatQuantityChange);
        if (Status == BillingOperationStatus.Completed)
        {
            return false;
        }

        if (Status != BillingOperationStatus.Pending)
        {
            throw new InvalidOperationException(
                "Only a pending seat-quantity change can be completed.");
        }

        ClosedAt = ValidateClosingTime(completedAt, "completion");
        Status = BillingOperationStatus.Completed;
        SeatQuantityOutcome = SeatQuantityChangeOutcome.Applied;
        return true;
    }

    public bool TryFailSeatQuantityChange(
        SeatQuantityChangeOutcome outcome,
        DateTimeOffset failedAt)
    {
        EnsureKind(BillingOperationKind.SeatQuantityChange);
        if (outcome is not SeatQuantityChangeOutcome.Superseded
            and not SeatQuantityChangeOutcome.ProviderRejected)
        {
            throw new ArgumentOutOfRangeException(nameof(outcome));
        }

        if (Status == BillingOperationStatus.Failed)
        {
            if (SeatQuantityOutcome != outcome)
            {
                throw new InvalidOperationException(
                    "A failed seat-quantity change cannot change its outcome.");
            }

            return false;
        }

        if (Status != BillingOperationStatus.Pending)
        {
            throw new InvalidOperationException(
                "Only a pending seat-quantity change can be failed.");
        }

        ClosedAt = ValidateClosingTime(failedAt, "failure");
        Status = BillingOperationStatus.Failed;
        SeatQuantityOutcome = outcome;
        return true;
    }

    public bool TryStartSeatQuantityProviderMutationReplay(DateTimeOffset providerObservedAt)
    {
        EnsureKind(BillingOperationKind.SeatQuantityChange);
        if (Status != BillingOperationStatus.Pending)
        {
            throw new InvalidOperationException(
                "Only a pending seat-quantity change can start provider mutation replay.");
        }

        if (ProviderMutationReplayStartedAt is not null)
        {
            return false;
        }

        ProviderMutationReplayStartedAt = providerObservedAt.ToUniversalTime();
        return true;
    }

    private DateTimeOffset ValidateClosingTime(
        DateTimeOffset closingTime,
        string description)
    {
        var utcClosingTime = closingTime.ToUniversalTime();
        if (utcClosingTime < CreatedAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(closingTime),
                $"A billing-operation {description} cannot predate its operation.");
        }

        return utcClosingTime;
    }

    private void EnsureKind(BillingOperationKind expectedKind)
    {
        if (Kind != expectedKind)
        {
            throw new InvalidOperationException(
                $"A {Kind} operation cannot perform a {expectedKind} transition.");
        }
    }
}
