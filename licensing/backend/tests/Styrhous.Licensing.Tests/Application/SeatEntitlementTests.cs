using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Billing;

namespace Styrhous.Licensing.Tests.Application;

[TestFixture]
public sealed class SeatEntitlementTests
{
    private static readonly Guid SeatId = Guid.CreateVersion7();
    private static readonly Guid BillingAccountId = Guid.CreateVersion7();
    private static readonly DateTimeOffset TrialStart =
        new(2026, 8, 30, 10, 0, 0, TimeSpan.Zero);
    private static readonly DateTimeOffset TrialEnd = TrialStart.AddDays(30);

    [TestCase(-1, EntitlementState.Evaluation, EntitlementReasonCodes.TrialNotStarted, false)]
    [TestCase(0, EntitlementState.Trial, EntitlementReasonCodes.ActiveTrial, true)]
    [TestCase(29, EntitlementState.Trial, EntitlementReasonCodes.ActiveTrial, true)]
    [TestCase(30, EntitlementState.Evaluation, EntitlementReasonCodes.TrialExpired, false)]
    public void TrialWindowUsesInclusiveStartAndExclusiveEnd(
        int daysAfterStart,
        EntitlementState expectedState,
        string expectedReasonCode,
        bool expectedEligibility)
    {
        var source = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            TrialStart,
            TrialEnd);

        var entitlement = SeatEntitlement.Resolve(source, TrialStart.AddDays(daysAfterStart));

        Assert.Multiple(() =>
        {
            Assert.That(entitlement.SeatId, Is.EqualTo(SeatId));
            Assert.That(entitlement.BillingAccountId, Is.EqualTo(BillingAccountId));
            Assert.That(entitlement.State, Is.EqualTo(expectedState));
            Assert.That(entitlement.ReasonCode, Is.EqualTo(expectedReasonCode));
            Assert.That(entitlement.IsEligible, Is.EqualTo(expectedEligibility));
            Assert.That(entitlement.ValidFrom, Is.EqualTo(TrialStart));
            Assert.That(entitlement.ValidUntil, Is.EqualTo(TrialEnd));
        });
    }

    [Test]
    public void SeatWithoutTrialHasNoValidEntitlementOrValidityWindow()
    {
        var source = new SeatEntitlementSource(SeatId, BillingAccountId, null, null);

        var entitlement = SeatEntitlement.Resolve(source, TrialStart);

        Assert.Multiple(() =>
        {
            Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(entitlement.ReasonCode, Is.EqualTo(EntitlementReasonCodes.NoValidEntitlement));
            Assert.That(entitlement.IsEligible, Is.False);
            Assert.That(entitlement.ValidFrom, Is.Null);
            Assert.That(entitlement.ValidUntil, Is.Null);
        });
    }

    [Test]
    public void DisabledProductAccessOverridesAnOtherwiseActiveEntitlement()
    {
        var source = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            TrialStart,
            TrialEnd,
            ProductAccessEnabled: false);

        var entitlement = SeatEntitlement.Resolve(source, TrialStart.AddDays(1));

        Assert.Multiple(() =>
        {
            Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(
                entitlement.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.ProductSeatNotAssigned));
            Assert.That(entitlement.IsEligible, Is.False);
            Assert.That(entitlement.ValidFrom, Is.Null);
            Assert.That(entitlement.ValidUntil, Is.Null);
        });
    }

    [TestCase(true, false)]
    [TestCase(false, true)]
    public void TrialWindowRequiresBothBoundaries(bool hasStart, bool hasEnd)
    {
        var source = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            hasStart ? TrialStart : null,
            hasEnd ? TrialEnd : null);

        Assert.That(
            () => SeatEntitlement.Resolve(source, TrialStart),
            Throws.TypeOf<InvalidOperationException>());
    }

    [Test]
    public void TrialWindowRequiresEndAfterStart()
    {
        var source = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            TrialStart,
            TrialStart);

        Assert.That(
            () => SeatEntitlement.Resolve(source, TrialStart),
            Throws.TypeOf<InvalidOperationException>());
    }

    [TestCase("seatId", "36c1e76c-8841-44f9-8864-6e7ffe632ef1")]
    [TestCase("seatId", TestIdentifiers.Version7WithNonRfcVariantText)]
    [TestCase("billingAccountId", "36c1e76c-8841-44f9-8864-6e7ffe632ef1")]
    [TestCase("billingAccountId", TestIdentifiers.Version7WithNonRfcVariantText)]
    public void SourcePreservesExistingRelationshipIdentifiers(string relationship, string value)
    {
        var source = new SeatEntitlementSource(
            relationship == "seatId" ? Guid.Parse(value) : SeatId,
            relationship == "billingAccountId" ? Guid.Parse(value) : BillingAccountId,
            TrialStart,
            TrialEnd);

        var result = SeatEntitlement.Resolve(source, TrialStart);
        Assert.That(relationship == "seatId" ? result.SeatId : result.BillingAccountId,
            Is.EqualTo(Guid.Parse(value)));
    }

    [TestCase(
        CommercialSubscriptionStatus.Active,
        false,
        EntitlementState.Commercial,
        EntitlementReasonCodes.ActiveSubscription)]
    [TestCase(
        CommercialSubscriptionStatus.Active,
        true,
        EntitlementState.Commercial,
        EntitlementReasonCodes.SubscriptionCancelsAtPeriodEnd)]
    [TestCase(
        CommercialSubscriptionStatus.PastDue,
        false,
        EntitlementState.Grace,
        EntitlementReasonCodes.SubscriptionPastDue)]
    [TestCase(
        CommercialSubscriptionStatus.Unpaid,
        false,
        EntitlementState.Evaluation,
        EntitlementReasonCodes.SubscriptionInactive)]
    [TestCase(
        CommercialSubscriptionStatus.Paused,
        false,
        EntitlementState.Evaluation,
        EntitlementReasonCodes.SubscriptionInactive)]
    [TestCase(
        CommercialSubscriptionStatus.IncompleteExpired,
        false,
        EntitlementState.Evaluation,
        EntitlementReasonCodes.SubscriptionInactive)]
    [TestCase(
        CommercialSubscriptionStatus.Incomplete,
        false,
        EntitlementState.Evaluation,
        EntitlementReasonCodes.SubscriptionInactive)]
    [TestCase(
        CommercialSubscriptionStatus.Trialing,
        false,
        EntitlementState.Evaluation,
        EntitlementReasonCodes.SubscriptionInactive)]
    [TestCase(
        CommercialSubscriptionStatus.Canceled,
        false,
        EntitlementState.Evaluation,
        EntitlementReasonCodes.SubscriptionInactive)]
    public void SubscriptionStatusControlsCommercialEligibility(
        CommercialSubscriptionStatus status,
        bool cancelsAtPeriodEnd,
        EntitlementState expectedState,
        string expectedReasonCode)
    {
        var source = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            null,
            null,
            Commercial(
                status,
                TrialStart.AddDays(1),
                TrialStart.AddDays(32),
                cancelsAtPeriodEnd));

        var entitlement = SeatEntitlement.Resolve(source, TrialStart.AddDays(2));

        Assert.Multiple(() =>
        {
            Assert.That(entitlement.State, Is.EqualTo(expectedState));
            Assert.That(entitlement.ReasonCode, Is.EqualTo(expectedReasonCode));
            Assert.That(
                entitlement.IsEligible,
                Is.EqualTo(expectedState is not EntitlementState.Evaluation));
            Assert.That(entitlement.ValidFrom, Is.EqualTo(TrialStart.AddDays(1)));
            Assert.That(entitlement.ValidUntil, Is.EqualTo(TrialStart.AddDays(32)));
        });
    }

    [TestCase(-1, EntitlementReasonCodes.SubscriptionNotStarted)]
    [TestCase(31, EntitlementReasonCodes.SubscriptionRenewalPending)]
    public void ActiveSubscriptionUsesHalfOpenPaidPeriod(
        int daysFromPeriodStart,
        string expectedReasonCode)
    {
        var periodStart = TrialStart.AddDays(1);
        var source = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            null,
            null,
            Commercial(
                CommercialSubscriptionStatus.Active,
                periodStart,
                periodStart.AddDays(31)));

        var entitlement = SeatEntitlement.Resolve(
            source,
            periodStart.AddDays(daysFromPeriodStart));

        Assert.Multiple(() =>
        {
            Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(entitlement.ReasonCode, Is.EqualTo(expectedReasonCode));
            Assert.That(entitlement.IsEligible, Is.False);
        });
    }

    [TestCase(CommercialSubscriptionStatus.Active, false, true, true, EntitlementReasonCodes.SubscriptionRenewalPending)]
    [TestCase(CommercialSubscriptionStatus.PastDue, false, true, true, EntitlementReasonCodes.SubscriptionRenewalPending)]
    [TestCase(CommercialSubscriptionStatus.Active, true, true, true, EntitlementReasonCodes.SubscriptionExpired)]
    [TestCase(CommercialSubscriptionStatus.Canceled, false, true, true, EntitlementReasonCodes.SubscriptionExpired)]
    [TestCase(CommercialSubscriptionStatus.Unpaid, false, true, true, EntitlementReasonCodes.SubscriptionExpired)]
    [TestCase(CommercialSubscriptionStatus.Paused, false, true, true, EntitlementReasonCodes.SubscriptionExpired)]
    [TestCase(CommercialSubscriptionStatus.Active, false, false, true, EntitlementReasonCodes.SubscriptionSeatCapacityExceeded)]
    [TestCase(CommercialSubscriptionStatus.Active, false, true, false, EntitlementReasonCodes.ProductSeatNotAssigned)]
    public void RenewalPendingNeverGrantsAccessOrRetainsDisabledOrUnfundedSeats(CommercialSubscriptionStatus status,
        bool cancel, bool funded, bool enabled, string reason)
    {
        var source = new SeatEntitlementSource(SeatId, BillingAccountId, null, null,
            new CommercialSeatEntitlementSource(status, TrialStart, TrialEnd, cancel, funded), enabled);
        var entitlement = SeatEntitlement.Resolve(source, TrialEnd);
        Assert.That(entitlement.IsEligible, Is.False);
        Assert.That(entitlement.ReasonCode, Is.EqualTo(reason));
    }

    [Test]
    public void CommercialSubscriptionTakesPrecedenceOverActiveTrial()
    {
        var source = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            TrialStart,
            TrialEnd,
            Commercial(
                CommercialSubscriptionStatus.Active,
                TrialStart,
                TrialEnd.AddDays(1)));

        var entitlement = SeatEntitlement.Resolve(source, TrialStart.AddDays(1));

        Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Commercial));
    }

    [Test]
    public void UnfundedCommercialSeatDoesNotFallBackToActiveTrial()
    {
        var source = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            TrialStart,
            TrialEnd,
            Commercial(
                CommercialSubscriptionStatus.Active,
                TrialStart,
                TrialEnd.AddDays(1),
                isSeatFunded: false));

        var entitlement = SeatEntitlement.Resolve(source, TrialStart.AddDays(1));

        Assert.Multiple(() =>
        {
            Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(
                entitlement.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.SubscriptionSeatCapacityExceeded));
            Assert.That(entitlement.IsEligible, Is.False);
            Assert.That(entitlement.ValidFrom, Is.EqualTo(TrialStart));
            Assert.That(entitlement.ValidUntil, Is.EqualTo(TrialEnd.AddDays(1)));
        });
    }

    [Test]
    public void InactiveSubscriptionDoesNotSuppressActiveInternalTrial()
    {
        var source = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            TrialStart,
            TrialEnd,
            Commercial(
                CommercialSubscriptionStatus.Incomplete,
                TrialStart,
                TrialEnd,
                isSeatFunded: false));

        var entitlement = SeatEntitlement.Resolve(source, TrialStart.AddDays(1));

        Assert.Multiple(() =>
        {
            Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Trial));
            Assert.That(entitlement.ReasonCode, Is.EqualTo(EntitlementReasonCodes.ActiveTrial));
        });
    }

    [Test]
    public void SubscriptionProjectionRejectsInvalidStatusAndPeriod()
    {
        var invalidStatus = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            TrialStart,
            TrialEnd,
            Commercial(
                (CommercialSubscriptionStatus)int.MaxValue,
                TrialStart,
                TrialEnd));
        var invalidPeriod = new SeatEntitlementSource(
            SeatId,
            BillingAccountId,
            TrialStart,
            TrialEnd,
            Commercial(
                CommercialSubscriptionStatus.Active,
                TrialStart,
                TrialStart));

        Assert.Multiple(() =>
        {
            Assert.That(
                () => SeatEntitlement.Resolve(invalidStatus, TrialStart),
                Throws.TypeOf<InvalidOperationException>());
            Assert.That(
                () => SeatEntitlement.Resolve(invalidPeriod, TrialStart),
                Throws.TypeOf<InvalidOperationException>());
        });
    }

    private static CommercialSeatEntitlementSource Commercial(
        CommercialSubscriptionStatus status,
        DateTimeOffset periodStartedAt,
        DateTimeOffset periodEndsAt,
        bool cancelsAtPeriodEnd = false,
        bool isSeatFunded = true)
    {
        return new(
            status,
            periodStartedAt,
            periodEndsAt,
            cancelsAtPeriodEnd,
            isSeatFunded);
    }
}
