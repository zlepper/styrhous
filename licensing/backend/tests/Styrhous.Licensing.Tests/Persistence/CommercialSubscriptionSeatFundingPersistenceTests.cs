using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class CommercialSubscriptionSeatFundingPersistenceTests
{
    private static readonly Guid EarlierSeatId =
        Guid.Parse("01917f8b-6000-7000-8000-000000000001");

    private static readonly Guid LaterSeatId =
        Guid.Parse("01917f8b-6000-7000-8000-000000000002");

    [Test]
    public async Task FundingPrioritizesOwnerThenSeatCreationTimeThenIdentifier()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var founder = await SignUpAsync(database, "funding-founder", "founder@example.com");
        var earlierIdentifierMember = await SignUpAsync(
            database,
            "funding-earlier-id",
            "earlier-id@example.com");
        var laterIdentifierMember = await SignUpAsync(
            database,
            "funding-later-id",
            "later-id@example.com");
        var currentOwner = await SignUpAsync(
            database,
            "funding-current-owner",
            "current-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            founder.UserId,
            "Funding Order",
            SignupTime.AddDays(1));
        var tiedCreatedAt = SignupTime.AddDays(2);
        await AddOrganizationMemberAsync(
            database,
            organization,
            earlierIdentifierMember.UserId,
            OrganizationRole.Member,
            seatId: EarlierSeatId,
            joinedAt: tiedCreatedAt);
        await AddOrganizationMemberAsync(
            database,
            organization,
            laterIdentifierMember.UserId,
            OrganizationRole.Member,
            seatId: LaterSeatId,
            joinedAt: tiedCreatedAt);
        var currentOwnerSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            currentOwner.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(3));
        await using (var ownershipContext = database.CreateContext())
        {
            await SetRoleAsync(
                ownershipContext,
                organization.OrganizationId,
                founder.UserId,
                OrganizationRole.Member);
            await SetRoleAsync(
                ownershipContext,
                organization.OrganizationId,
                currentOwner.UserId,
                OrganizationRole.Owner);
        }
        await ProjectAsync(database, organization.BillingAccountId, seatQuantity: 3);

        var founderEntitlement = await ResolveOrganizationEntitlementAsync(
            database,
            founder.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(31));
        var currentOwnerEntitlement = await ResolveOrganizationEntitlementAsync(
            database,
            currentOwner.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(31));
        var earlierIdentifierEntitlement = await ResolveOrganizationEntitlementAsync(
            database,
            earlierIdentifierMember.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(31));
        var laterIdentifierEntitlement = await ResolveOrganizationEntitlementAsync(
            database,
            laterIdentifierMember.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(31));

        Assert.Multiple(() =>
        {
            AssertCommercial(currentOwnerEntitlement, currentOwnerSeat.SeatId);
            AssertCommercial(founderEntitlement, organization.SeatId);
            AssertCommercial(earlierIdentifierEntitlement, EarlierSeatId);
            AssertCapacityExceeded(laterIdentifierEntitlement, LaterSeatId);
        });
    }

    [Test]
    public async Task DisabledHigherPrioritySeatDoesNotConsumeCommercialCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "funding-disabled-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "funding-enabled-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Dormant Seat Funding",
            SignupTime.AddDays(1));
        var memberSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));
        await using (var context = database.CreateContext())
        {
            await context.Seats
                .Where(seat => seat.Id == organization.SeatId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    seat => seat.ProductAccessEnabled,
                    false));
        }
        await ProjectAsync(database, organization.BillingAccountId, seatQuantity: 1);

        var ownerEntitlement = await ResolveOrganizationEntitlementAsync(
            database,
            owner.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(31));
        var memberEntitlement = await ResolveOrganizationEntitlementAsync(
            database,
            member.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(31));

        Assert.Multiple(() =>
        {
            Assert.That(ownerEntitlement.SeatId, Is.EqualTo(organization.SeatId));
            Assert.That(ownerEntitlement.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(
                ownerEntitlement.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.ProductSeatNotAssigned));
            Assert.That(ownerEntitlement.IsEligible, Is.False);
            Assert.That(ownerEntitlement.ValidFrom, Is.Null);
            Assert.That(ownerEntitlement.ValidUntil, Is.Null);
            AssertCommercial(memberEntitlement, memberSeat.SeatId);
        });
    }

    [Test]
    public async Task EntitlementListUsesOneSnapshotAcrossRowsAndFundingPriority()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var founder = await SignUpAsync(
            database,
            "funding-snapshot-founder",
            "founder@example.com");
        var member = await SignUpAsync(
            database,
            "funding-snapshot-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            founder.UserId,
            "Funding Snapshot",
            SignupTime.AddDays(1));
        var memberSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));
        await ProjectAsync(database, organization.BillingAccountId, seatQuantity: 1);
        var ownerQueryGate = new DatabaseCommandGate();
        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, SignupTime.AddDays(31), interceptors: [
            new DatabaseCommandGateInterceptor(ownerQueryGate, "FROM organizations")]);
        var snapshotTask = entitlementTest.Service
            .ListForUserAsync(founder.UserId);

        await ownerQueryGate.WaitUntilReachedAsync();
        try
        {
            await using var ownershipContext = database.CreateContext();
            await SetRoleAsync(
                ownershipContext,
                organization.OrganizationId,
                founder.UserId,
                OrganizationRole.Member);
            await SetRoleAsync(
                ownershipContext,
                organization.OrganizationId,
                member.UserId,
                OrganizationRole.Owner);
        }
        finally
        {
            ownerQueryGate.Release();
        }

        var snapshotEntitlement = (await snapshotTask).Single(
            entitlement => entitlement.BillingAccountId == organization.BillingAccountId);
        var liveFounderEntitlement = await ResolveOrganizationEntitlementAsync(
            database,
            founder.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(31));
        var liveMemberEntitlement = await ResolveOrganizationEntitlementAsync(
            database,
            member.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(31));

        Assert.Multiple(() =>
        {
            AssertCommercial(snapshotEntitlement, organization.SeatId);
            AssertCapacityExceeded(liveFounderEntitlement, organization.SeatId);
            AssertCommercial(liveMemberEntitlement, memberSeat.SeatId);
        });
    }

    private static Task<int> SetRoleAsync(
        LicensingDbContext context,
        Guid organizationId,
        Guid userId,
        OrganizationRole role)
    {
        return context.OrganizationMemberships
            .Where(membership =>
                membership.OrganizationId == organizationId
                && membership.UserId == userId)
            .ExecuteUpdateAsync(setters => setters.SetProperty(
                membership => membership.Role,
                role));
    }

    [Test]
    public async Task QuantityReductionLeavesOnlyTheHighestPrioritySeatsFunded()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "funding-reduction-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "funding-reduction-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Funding Reduction",
            SignupTime.AddDays(1));
        var memberSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime);
        await ProjectAsync(database, organization.BillingAccountId, seatQuantity: 2);

        var fundedBeforeReduction = await ResolveOrganizationEntitlementAsync(
            database,
            member.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(31));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            seatQuantity: 1,
            projectedAt: SignupTime.AddDays(32),
            expectedStatus: CommercialSubscriptionProjectionStatus.Updated);
        var ownerAfterReduction = await ResolveOrganizationEntitlementAsync(
            database,
            owner.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(32));
        var memberAfterReduction = await ResolveOrganizationEntitlementAsync(
            database,
            member.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(32));

        Assert.Multiple(() =>
        {
            AssertCommercial(fundedBeforeReduction, memberSeat.SeatId);
            AssertCommercial(ownerAfterReduction, organization.SeatId);
            AssertCapacityExceeded(memberAfterReduction, memberSeat.SeatId);
        });
    }

    [Test]
    public async Task InactiveSubscriptionStillFallsBackToTheTransferredTrialForEverySeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "funding-trial-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "funding-trial-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Funding Trial Fallback",
            SignupTime.AddDays(1));
        var memberSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            seatQuantity: 1,
            status: CommercialSubscriptionStatus.Unpaid);

        var ownerEntitlement = await ResolveOrganizationEntitlementAsync(
            database,
            owner.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(2));
        var memberEntitlement = await ResolveOrganizationEntitlementAsync(
            database,
            member.UserId,
            organization.BillingAccountId,
            SignupTime.AddDays(2));

        Assert.Multiple(() =>
        {
            AssertTrial(ownerEntitlement, organization.SeatId);
            AssertTrial(memberEntitlement, memberSeat.SeatId);
        });
    }

    [Test]
    public async Task QuantityReductionStopsActivationAndChecksWithoutTouchingLastSeen()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "funding-device-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "funding-device-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Funding Device",
            SignupTime.AddDays(1));
        var memberSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));
        await ProjectAsync(database, organization.BillingAccountId, seatQuantity: 2);
        var activation = await ActivateAsync(
            database,
            member.UserId,
            memberSeat.SeatId,
            installationNumber: 1,
            SignupTime.AddDays(31));
        Assert.That(activation.Status, Is.EqualTo(DeviceActivationStatus.Activated));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            seatQuantity: 1,
            projectedAt: SignupTime.AddDays(32),
            expectedStatus: CommercialSubscriptionProjectionStatus.Updated);

        await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, SignupTime.AddDays(32));
        var check = await checkTest.Service
            .CheckAsync(member.UserId, activation.ActivationId!.Value);
        var secondActivation = await ActivateAsync(
            database,
            member.UserId,
            memberSeat.SeatId,
            installationNumber: 2,
            SignupTime.AddDays(32));
        await using var verificationContext = database.CreateContext();
        var persistedActivation = await verificationContext.DeviceActivations.SingleAsync();

        Assert.Multiple(() =>
        {
            Assert.That(check.Status, Is.EqualTo(DeviceEntitlementCheckStatus.Ineligible));
            Assert.That(
                check.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.SubscriptionSeatCapacityExceeded));
            Assert.That(check.Entitlement!.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(
                secondActivation.Status,
                Is.EqualTo(DeviceActivationStatus.SeatNotEligible));
            Assert.That(persistedActivation.LastSeenAt, Is.EqualTo(SignupTime.AddDays(31)));
            Assert.That(verificationContext.DeviceActivations, Has.Exactly(1).Items);
        });
    }

    private static async Task ProjectAsync(
        PostgresTestDatabase database,
        Guid billingAccountId,
        int seatQuantity,
        CommercialSubscriptionStatus status = CommercialSubscriptionStatus.Active,
        DateTimeOffset? projectedAt = null,
        CommercialSubscriptionProjectionStatus expectedStatus =
            CommercialSubscriptionProjectionStatus.Created)
    {
        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime);
        var result = await projectionTest.Service
            .ApplyAsync(
                billingAccountId,
                new CommercialSubscriptionProjection(
                    "cus_funding",
                    "sub_funding",
                    "price_funding",
                    status,
                    seatQuantity,
                    cancelAtPeriodEnd: false,
                    SignupTime.AddDays(30),
                    SignupTime.AddDays(60),
                    projectedAt ?? SignupTime.AddDays(30)));
        Assert.That(result.Status, Is.EqualTo(expectedStatus));
    }

    private static async Task<SeatEntitlement> ResolveOrganizationEntitlementAsync(
        PostgresTestDatabase database,
        Guid userId,
        Guid billingAccountId,
        DateTimeOffset observedAt)
    {
        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, observedAt);
        var entitlements = await entitlementTest.Service
            .ListForUserAsync(userId);
        return entitlements.Single(
            entitlement => entitlement.BillingAccountId == billingAccountId);
    }

    private static void AssertCommercial(SeatEntitlement entitlement, Guid seatId)
    {
        Assert.That(entitlement.SeatId, Is.EqualTo(seatId));
        Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Commercial));
        Assert.That(entitlement.ReasonCode, Is.EqualTo(EntitlementReasonCodes.ActiveSubscription));
        Assert.That(entitlement.IsEligible, Is.True);
        Assert.That(entitlement.ValidFrom, Is.EqualTo(SignupTime.AddDays(30)));
        Assert.That(entitlement.ValidUntil, Is.EqualTo(SignupTime.AddDays(60)));
    }

    private static void AssertCapacityExceeded(SeatEntitlement entitlement, Guid seatId)
    {
        Assert.That(entitlement.SeatId, Is.EqualTo(seatId));
        Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Evaluation));
        Assert.That(
            entitlement.ReasonCode,
            Is.EqualTo(EntitlementReasonCodes.SubscriptionSeatCapacityExceeded));
        Assert.That(entitlement.IsEligible, Is.False);
        Assert.That(entitlement.ValidFrom, Is.EqualTo(SignupTime.AddDays(30)));
        Assert.That(entitlement.ValidUntil, Is.EqualTo(SignupTime.AddDays(60)));
    }

    private static void AssertTrial(SeatEntitlement entitlement, Guid seatId)
    {
        Assert.That(entitlement.SeatId, Is.EqualTo(seatId));
        Assert.That(entitlement.State, Is.EqualTo(EntitlementState.Trial));
        Assert.That(entitlement.ReasonCode, Is.EqualTo(EntitlementReasonCodes.ActiveTrial));
        Assert.That(entitlement.IsEligible, Is.True);
        Assert.That(entitlement.ValidFrom, Is.EqualTo(SignupTime));
        Assert.That(entitlement.ValidUntil, Is.EqualTo(SignupTime.AddDays(30)));
    }
}
