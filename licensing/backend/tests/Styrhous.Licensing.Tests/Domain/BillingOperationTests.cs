using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingOperationTests
{
    private static readonly DateTimeOffset CreatedAt =
        new(2026, 9, 1, 10, 0, 0, TimeSpan.FromHours(2));

    [TestCase(BillingCadence.Monthly)]
    [TestCase(BillingCadence.Annual)]
    public void InitialCheckoutStartsAsUuid7PendingOperation(BillingCadence cadence)
    {
        var billingAccountId = Guid.CreateVersion7();
        var actorUserId = Guid.CreateVersion7();

        var operation = BillingOperation.StartInitialCheckout(
            billingAccountId,
            actorUserId,
            cadence,
            seatQuantity: 12,
            CreatedAt);

        Assert.Multiple(() =>
        {
            Assert.That(operation.Id.Version, Is.EqualTo(7));
            Assert.That(operation.BillingAccountId, Is.EqualTo(billingAccountId));
            Assert.That(operation.ActorUserId, Is.EqualTo(actorUserId));
            Assert.That(operation.Kind, Is.EqualTo(BillingOperationKind.InitialCheckout));
            Assert.That(operation.Cadence, Is.EqualTo(cadence));
            Assert.That(operation.PreviousSeatQuantity, Is.Null);
            Assert.That(operation.SeatQuantity, Is.EqualTo(12));
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Pending));
            Assert.That(operation.CreatedAt, Is.EqualTo(CreatedAt.ToUniversalTime()));
            Assert.That(
                operation.ExpiresAt,
                Is.EqualTo(CreatedAt.ToUniversalTime().Add(BillingOperation.CheckoutLifetime)));
            Assert.That(operation.ClosedAt, Is.Null);
            Assert.That(operation.ProviderSessionRecordedAt, Is.Null);
            Assert.That(operation.ExternalSessionId, Is.Null);
        });
    }

    [Test]
    public void SeatQuantityChangeHasImmutableUuid7PendingStateWithoutCheckoutFields()
    {
        var billingAccountId = Guid.CreateVersion7();
        var actorUserId = Guid.CreateVersion7();

        var operation = BillingOperation.StartSeatQuantityChange(
            billingAccountId,
            actorUserId,
            previousSeatQuantity: 3,
            seatQuantity: int.MaxValue,
            CreatedAt);

        Assert.Multiple(() =>
        {
            Assert.That(operation.Id.Version, Is.EqualTo(7));
            Assert.That(operation.BillingAccountId, Is.EqualTo(billingAccountId));
            Assert.That(operation.ActorUserId, Is.EqualTo(actorUserId));
            Assert.That(operation.Kind, Is.EqualTo(BillingOperationKind.SeatQuantityChange));
            Assert.That(operation.Cadence, Is.Null);
            Assert.That(operation.PreviousSeatQuantity, Is.EqualTo(3));
            Assert.That(operation.SeatQuantity, Is.EqualTo(int.MaxValue));
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Pending));
            Assert.That(operation.CreatedAt, Is.EqualTo(CreatedAt.ToUniversalTime()));
            Assert.That(operation.ExpiresAt, Is.Null);
            Assert.That(operation.ClosedAt, Is.Null);
            Assert.That(operation.ProviderSessionRecordedAt, Is.Null);
            Assert.That(operation.ExternalSessionId, Is.Null);
        });
    }

    [TestCase(0, 2)]
    [TestCase(-1, 2)]
    [TestCase(2, 0)]
    [TestCase(2, -1)]
    [TestCase(2, 2)]
    public void SeatQuantityChangeRequiresDistinctPositiveQuantities(
        int previousSeatQuantity,
        int seatQuantity)
    {
        Assert.That(
            () => BillingOperation.StartSeatQuantityChange(
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                previousSeatQuantity,
                seatQuantity,
                CreatedAt),
            Throws.InstanceOf<ArgumentException>());
    }

    [Test]
    public void SeatQuantityChangeCompletionAndFailureAreKindSpecificAndIdempotent()
    {
        var completed = BillingOperation.StartSeatQuantityChange(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            previousSeatQuantity: 2,
            seatQuantity: 5,
            CreatedAt);
        var failed = BillingOperation.StartSeatQuantityChange(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            previousSeatQuantity: 5,
            seatQuantity: 2,
            CreatedAt);
        var closedAt = CreatedAt.AddMinutes(1);

        Assert.Multiple(() =>
        {
            Assert.That(completed.TryCompleteSeatQuantityChange(closedAt), Is.True);
            Assert.That(
                completed.TryCompleteSeatQuantityChange(closedAt.AddMinutes(1)),
                Is.False);
            Assert.That(completed.Status, Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(completed.ClosedAt, Is.EqualTo(closedAt.ToUniversalTime()));
            Assert.That(
                completed.SeatQuantityOutcome,
                Is.EqualTo(SeatQuantityChangeOutcome.Applied));

            Assert.That(
                failed.TryFailSeatQuantityChange(
                    SeatQuantityChangeOutcome.Superseded,
                    closedAt),
                Is.True);
            Assert.That(
                failed.TryFailSeatQuantityChange(
                    SeatQuantityChangeOutcome.Superseded,
                    closedAt.AddMinutes(1)),
                Is.False);
            Assert.That(failed.Status, Is.EqualTo(BillingOperationStatus.Failed));
            Assert.That(failed.ClosedAt, Is.EqualTo(closedAt.ToUniversalTime()));
            Assert.That(
                failed.SeatQuantityOutcome,
                Is.EqualTo(SeatQuantityChangeOutcome.Superseded));

            Assert.That(
                () => completed.TryRecordProviderSession("cs_wrong", closedAt),
                Throws.InvalidOperationException);
        });
    }

    [Test]
    public void SeatQuantityProviderMutationReplayRecordsOnlyItsProviderClockStart()
    {
        var operation = BillingOperation.StartSeatQuantityChange(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            previousSeatQuantity: 2,
            seatQuantity: 5,
            CreatedAt);
        var replayStartedAt = CreatedAt.AddMinutes(5);

        Assert.Multiple(() =>
        {
            Assert.That(
                operation.TryStartSeatQuantityProviderMutationReplay(replayStartedAt),
                Is.True);
            Assert.That(
                operation.TryStartSeatQuantityProviderMutationReplay(
                    CreatedAt.AddHours(-1)),
                Is.False);
            Assert.That(
                operation.ProviderMutationReplayStartedAt,
                Is.EqualTo(replayStartedAt.ToUniversalTime()));
        });
    }

    [TestCase(0)]
    [TestCase(-1)]
    public void InitialCheckoutRequiresPositiveSeatQuantity(int seatQuantity)
    {
        Assert.That(
            () => BillingOperation.StartInitialCheckout(
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                BillingCadence.Monthly,
                seatQuantity,
                CreatedAt),
            Throws.TypeOf<ArgumentOutOfRangeException>());
    }

    [Test]
    public void InitialCheckoutPreservesExistingRelationshipIdentifiers()
    {
        var accountId = Guid.NewGuid();
        var userId = Guid.NewGuid();
        var operation = BillingOperation.StartInitialCheckout(accountId, userId,
            BillingCadence.Monthly, seatQuantity: 1, CreatedAt);
        Assert.Multiple(() =>
        {
            Assert.That(operation.BillingAccountId, Is.EqualTo(accountId));
            Assert.That(operation.ActorUserId, Is.EqualTo(userId));
            Assert.That(operation.Id.Version, Is.EqualTo(7));
        });
    }

    [Test]
    public void ProviderSessionRecordingIsIdempotentOnlyForTheSameSession()
    {
        var operation = BillingOperation.StartInitialCheckout(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            BillingCadence.Annual,
            seatQuantity: 4,
            CreatedAt);
        var providerSessionRecordedAt = CreatedAt.AddMinutes(1);
        var checkoutExpiry = operation.ExpiresAt
            ?? throw new InvalidOperationException("Checkout expiry is missing.");

        var firstCompletion = operation.TryRecordProviderSession(
            "cs_checkout",
            providerSessionRecordedAt);
        var repeatedCompletion = operation.TryRecordProviderSession(
            "cs_checkout",
            checkoutExpiry.AddMinutes(1));

        Assert.Multiple(() =>
        {
            Assert.That(firstCompletion, Is.True);
            Assert.That(repeatedCompletion, Is.False);
            Assert.That(
                operation.Status,
                Is.EqualTo(BillingOperationStatus.ProviderSessionCreated));
            Assert.That(operation.ExternalSessionId, Is.EqualTo("cs_checkout"));
            Assert.That(
                operation.ProviderSessionRecordedAt,
                Is.EqualTo(providerSessionRecordedAt.ToUniversalTime()));
            Assert.That(
                () => operation.TryRecordProviderSession(
                    "cs_different",
                    providerSessionRecordedAt.AddSeconds(2)),
                Throws.InvalidOperationException);
        });
    }

    [Test]
    public void ProviderSessionUsesLocalRecordingTimeWithoutCrossClockComparison()
    {
        var fractionalCreatedAt = CreatedAt.AddTicks(TimeSpan.TicksPerSecond - 1);
        var recordedAt = fractionalCreatedAt.AddMilliseconds(20);
        var operation = BillingOperation.StartInitialCheckout(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            BillingCadence.Monthly,
            seatQuantity: 1,
            fractionalCreatedAt);

        var recorded = operation.TryRecordProviderSession(
            "cs_same_second",
            recordedAt);

        Assert.Multiple(() =>
        {
            Assert.That(recorded, Is.True);
            Assert.That(operation.CreatedAt, Is.EqualTo(fractionalCreatedAt.ToUniversalTime()));
            Assert.That(
                operation.ProviderSessionRecordedAt,
                Is.EqualTo(recordedAt.ToUniversalTime()));
            Assert.That(
                operation.Status,
                Is.EqualTo(BillingOperationStatus.ProviderSessionCreated));
        });
    }

    [Test]
    public void CompletionRejectsInvalidProviderState()
    {
        var operation = BillingOperation.StartInitialCheckout(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            BillingCadence.Monthly,
            seatQuantity: 1,
            CreatedAt);

        Assert.Multiple(() =>
        {
            Assert.That(
                () => operation.TryRecordProviderSession(" ", CreatedAt.AddMinutes(1)),
                Throws.TypeOf<ArgumentException>());
            Assert.That(
                () => operation.TryRecordProviderSession(
                    new string('s', BillingOperation.MaximumExternalIdentifierLength + 1),
                    CreatedAt.AddMinutes(1)),
                Throws.TypeOf<ArgumentException>());
            Assert.That(
                () => operation.TryRecordProviderSession(
                    "cs_early",
                    CreatedAt.AddTicks(-1)),
                Throws.TypeOf<ArgumentOutOfRangeException>());
            Assert.That(
                () => operation.TryRecordProviderSession(
                    "cs_recorded_late",
                    operation.ExpiresAt!.Value),
                Throws.Nothing);
        });
    }

    [Test]
    public void CheckoutExpiresOnlyAfterItsRecordedProviderSessionExpires()
    {
        var operation = BillingOperation.StartInitialCheckout(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            BillingCadence.Monthly,
            seatQuantity: 1,
            CreatedAt);

        Assert.That(
            () => operation.TryExpireProviderSession(
                "cs_checkout",
                operation.ExpiresAt!.Value),
            Throws.InvalidOperationException);

        operation.TryRecordProviderSession("cs_checkout", CreatedAt.AddMinutes(1));
        var expiredAt = operation.ExpiresAt!.Value.AddSeconds(1);
        Assert.Multiple(() =>
        {
            Assert.That(
                operation.TryExpireProviderSession("cs_checkout", expiredAt),
                Is.True);
            Assert.That(
                operation.TryExpireProviderSession("cs_checkout", expiredAt.AddMinutes(1)),
                Is.False);
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Expired));
            Assert.That(operation.ClosedAt, Is.EqualTo(expiredAt.ToUniversalTime()));
            Assert.That(
                operation.TryRecordProviderSession(
                    "cs_checkout",
                    operation.ExpiresAt!.Value.AddMinutes(2)),
                Is.False);
        });
    }

    [Test]
    public void CheckoutCompletesOnlyAfterItsProviderSessionWasRecorded()
    {
        var operation = BillingOperation.StartInitialCheckout(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            BillingCadence.Monthly,
            seatQuantity: 1,
            CreatedAt);
        var recordedAt = CreatedAt.AddMinutes(1);
        var completedAt = recordedAt.AddMinutes(2);

        Assert.That(
            () => operation.TryCompleteProviderSession(completedAt),
            Throws.InvalidOperationException);
        operation.TryRecordProviderSession("cs_completed", recordedAt);

        Assert.Multiple(() =>
        {
            Assert.That(operation.TryCompleteProviderSession(completedAt), Is.True);
            Assert.That(
                operation.TryCompleteProviderSession(completedAt.AddMinutes(1)),
                Is.False);
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(operation.ClosedAt, Is.EqualTo(completedAt.ToUniversalTime()));
            Assert.That(operation.ExternalSessionId, Is.EqualTo("cs_completed"));
        });
    }

    [Test]
    public void DefiniteProviderRejectionFailsOnlyAPendingOperation()
    {
        var operation = BillingOperation.StartInitialCheckout(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            BillingCadence.Monthly,
            seatQuantity: 1,
            CreatedAt);
        var failedAt = CreatedAt.AddMinutes(31);

        Assert.Multiple(() =>
        {
            Assert.That(operation.TryFailProviderSessionCreation(failedAt), Is.True);
            Assert.That(
                operation.TryFailProviderSessionCreation(failedAt.AddMinutes(1)),
                Is.False);
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Failed));
            Assert.That(operation.ClosedAt, Is.EqualTo(failedAt.ToUniversalTime()));
            Assert.That(operation.ExternalSessionId, Is.Null);
        });
    }

}
