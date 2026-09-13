using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Tests.Domain;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class CommercialSubscriptionTests
{
    private static readonly DateTimeOffset PeriodStart =
        new(2026, 9, 1, 10, 0, 0, TimeSpan.Zero);

    [Test]
    public void ProjectionOwnsUuid7AndNormalizesUtcTimes()
    {
        var subscription = CommercialSubscription.Create(
            Guid.CreateVersion7(),
            new CommercialSubscriptionProjection(
                "cus_example",
                "sub_example",
                "price_example",
                CommercialSubscriptionStatus.Active,
                seatQuantity: 5,
                cancelAtPeriodEnd: false,
                PeriodStart.ToOffset(TimeSpan.FromHours(2)),
                PeriodStart.AddMonths(1).ToOffset(TimeSpan.FromHours(2)),
                PeriodStart.ToOffset(TimeSpan.FromHours(2))));

        Assert.Multiple(() =>
        {
            Assert.That(subscription.Id.Version, Is.EqualTo(7));
            Assert.That(subscription.BillingAccountId.Version, Is.EqualTo(7));
            Assert.That(subscription.SeatQuantity, Is.EqualTo(5));
            Assert.That(subscription.CurrentPeriodStartedAt, Is.EqualTo(PeriodStart));
            Assert.That(subscription.CurrentPeriodEndsAt, Is.EqualTo(PeriodStart.AddMonths(1)));
            Assert.That(subscription.ProjectedAt, Is.EqualTo(PeriodStart));
        });
    }

    [Test]
    public void ProjectionRequiresPositiveSeatQuantityAndValidPeriod()
    {
        Assert.Multiple(() =>
        {
            Assert.That(
                () => Create(seatQuantity: 0),
                Throws.TypeOf<ArgumentOutOfRangeException>());
            Assert.That(
                () => Create(periodEnd: PeriodStart),
                Throws.TypeOf<ArgumentOutOfRangeException>());
            Assert.That(
                () => Create(projectedAt: default(DateTimeOffset)),
                Throws.TypeOf<ArgumentOutOfRangeException>());
        });
    }

    [TestCase("customer", " ")]
    [TestCase("subscription", " ")]
    [TestCase("price", " ")]
    public void ProjectionRequiresExternalIdentifiers(string identifier, string value)
    {
        Assert.That(
            () => Create(
                customerId: identifier == "customer" ? value : "cus_example",
                subscriptionId: identifier == "subscription" ? value : "sub_example",
                priceId: identifier == "price" ? value : "price_example"),
            Throws.TypeOf<ArgumentException>());
    }

    [Test]
    public void ProjectionPreservesExistingBillingAccountAndRejectsUnknownStatus()
    {
        var billingAccountId = Guid.NewGuid();
        Assert.That(Create(billingAccountId: billingAccountId).BillingAccountId, Is.EqualTo(billingAccountId));
        Assert.That(
            () => Create(status: (CommercialSubscriptionStatus)int.MaxValue),
            Throws.TypeOf<ArgumentOutOfRangeException>()
                .With.Property(nameof(ArgumentOutOfRangeException.ParamName)).EqualTo("status"));
    }

    [Test]
    public void ProjectionEnforcesExternalIdentifierLengthBoundaries()
    {
        var exactMaximum = new string(
            'x',
            CommercialSubscription.MaximumExternalIdentifierLength);
        var subscription = Create(
            customerId: exactMaximum,
            subscriptionId: exactMaximum,
            priceId: exactMaximum);

        Assert.That(subscription.ExternalCustomerId, Has.Length.EqualTo(exactMaximum.Length));
        foreach (var identifier in new[] { "customer", "subscription", "price" })
        {
            Assert.That(
                () => Create(
                    customerId: identifier == "customer"
                        ? exactMaximum + "x"
                        : "cus_example",
                    subscriptionId: identifier == "subscription"
                        ? exactMaximum + "x"
                        : "sub_example",
                    priceId: identifier == "price"
                        ? exactMaximum + "x"
                        : "price_example"),
                Throws.InstanceOf<ArgumentException>());
        }
    }

    [Test]
    public void ProjectionTrimsExternalIdentifiers()
    {
        var subscription = Create(
            customerId: " cus_example ",
            subscriptionId: " sub_example ",
            priceId: " price_example ");

        Assert.Multiple(() =>
        {
            Assert.That(subscription.ExternalCustomerId, Is.EqualTo("cus_example"));
            Assert.That(subscription.ExternalSubscriptionId, Is.EqualTo("sub_example"));
            Assert.That(subscription.ExternalPriceId, Is.EqualTo("price_example"));
        });
    }

    [Test]
    public void ProjectionRejectsNullExternalIdentifiers()
    {
        foreach (var identifier in new[] { "customer", "subscription", "price" })
        {
            Assert.That(
                () => Create(
                    customerId: identifier == "customer" ? null! : "cus_example",
                    subscriptionId: identifier == "subscription" ? null! : "sub_example",
                    priceId: identifier == "price" ? null! : "price_example"),
                Throws.InstanceOf<ArgumentException>());
        }
    }

    [Test]
    public void NewerProjectionUpdatesInPlaceWithoutChangingOwnedIdentity()
    {
        var subscription = Create();
        var id = subscription.Id;

        var applied = subscription.TryApplyProjection(
            new CommercialSubscriptionProjection(
                "cus_replaced",
                "sub_example",
                "price_annual",
                CommercialSubscriptionStatus.PastDue,
                seatQuantity: 7,
                cancelAtPeriodEnd: true,
                PeriodStart.AddMonths(1),
                PeriodStart.AddMonths(2),
                PeriodStart.AddMinutes(1)));

        Assert.Multiple(() =>
        {
            Assert.That(applied, Is.True);
            Assert.That(subscription.Id, Is.EqualTo(id));
            Assert.That(subscription.ExternalCustomerId, Is.EqualTo("cus_replaced"));
            Assert.That(subscription.ExternalSubscriptionId, Is.EqualTo("sub_example"));
            Assert.That(subscription.ExternalPriceId, Is.EqualTo("price_annual"));
            Assert.That(subscription.Status, Is.EqualTo(CommercialSubscriptionStatus.PastDue));
            Assert.That(subscription.SeatQuantity, Is.EqualTo(7));
            Assert.That(subscription.CancelAtPeriodEnd, Is.True);
            Assert.That(subscription.ProjectedAt, Is.EqualTo(PeriodStart.AddMinutes(1)));
        });
    }

    [TestCase(0)]
    [TestCase(-1)]
    public void DuplicateAndStaleProjectionsAreIgnored(int minutesAfterCurrentProjection)
    {
        var subscription = Create();

        var applied = subscription.TryApplyProjection(
            new CommercialSubscriptionProjection(
                "cus_stale",
                "sub_stale",
                "price_stale",
                CommercialSubscriptionStatus.Canceled,
                seatQuantity: 9,
                cancelAtPeriodEnd: true,
                PeriodStart.AddMonths(1),
                PeriodStart.AddMonths(2),
                PeriodStart.AddMinutes(minutesAfterCurrentProjection)));

        Assert.Multiple(() =>
        {
            Assert.That(applied, Is.False);
            Assert.That(subscription.ExternalCustomerId, Is.EqualTo("cus_example"));
            Assert.That(subscription.ExternalSubscriptionId, Is.EqualTo("sub_example"));
            Assert.That(subscription.Status, Is.EqualTo(CommercialSubscriptionStatus.Active));
            Assert.That(subscription.SeatQuantity, Is.EqualTo(1));
            Assert.That(subscription.ProjectedAt, Is.EqualTo(PeriodStart));
        });
    }

    [Test]
    public void MutationResponseOutranksObservationAtTheSameProviderTime()
    {
        var subscription = CommercialSubscription.Create(
            Guid.CreateVersion7(),
            CreateProjection("initial", seatQuantity: 3),
            providerReadRevision: 1);
        Assert.That(
            subscription.TryApplyProjection(
                CreateProjection("mutation", seatQuantity: 5),
                providerReadRevision: 2,
                CommercialSubscriptionSnapshotKind.MutationResponse),
            Is.True);

        var applied = subscription.TryApplyProjection(
            CreateProjection("delayed-observation", seatQuantity: 3),
            providerReadRevision: 3,
            CommercialSubscriptionSnapshotKind.Observation);

        Assert.Multiple(() =>
        {
            Assert.That(applied, Is.False);
            Assert.That(subscription.SeatQuantity, Is.EqualTo(5));
            Assert.That(subscription.ExternalSubscriptionId, Is.EqualTo("sub_stable"));
            Assert.That(subscription.ProviderReadRevision, Is.EqualTo(2));
            Assert.That(
                subscription.ProviderSnapshotKind,
                Is.EqualTo(CommercialSubscriptionSnapshotKind.MutationResponse));
        });
    }

    private static CommercialSubscriptionProjection CreateProjection(
        string suffix,
        int seatQuantity)
    {
        return new(
            $"cus_{suffix}",
            "sub_stable",
            $"price_{suffix}",
            CommercialSubscriptionStatus.Active,
            seatQuantity,
            cancelAtPeriodEnd: false,
            PeriodStart,
            PeriodStart.AddMonths(1),
            PeriodStart);
    }

    private static CommercialSubscription Create(
        Guid? billingAccountId = null,
        int seatQuantity = 1,
        DateTimeOffset? periodEnd = null,
        string customerId = "cus_example",
        string subscriptionId = "sub_example",
        string priceId = "price_example",
        DateTimeOffset? projectedAt = null,
        CommercialSubscriptionStatus status = CommercialSubscriptionStatus.Active)
    {
        return CommercialSubscription.Create(
            billingAccountId ?? Guid.CreateVersion7(),
            new CommercialSubscriptionProjection(
                customerId,
                subscriptionId,
                priceId,
                status,
                seatQuantity,
                cancelAtPeriodEnd: false,
                PeriodStart,
                periodEnd ?? PeriodStart.AddMonths(1),
                projectedAt ?? PeriodStart));
    }
}
