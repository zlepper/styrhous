using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class CommercialSubscriptionSeatCapacityPersistenceTests
{
    [TestCase(CommercialSubscriptionStatus.Active)]
    [TestCase(CommercialSubscriptionStatus.PastDue)]
    public async Task EligiblePaidSubscriptionFundsPurchasedOrganizationSeatQuantity(
        CommercialSubscriptionStatus status)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, $"paid-capacity-{status}", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            $"Paid Capacity {status}",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            status,
            seatQuantity: 3,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(31));
        var service = serviceTest.Service;

        var first = await service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "first@example.com",
            OrganizationRole.Member);
        var second = await service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "second@example.com",
            OrganizationRole.Member);
        var overCapacity = await service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "third@example.com",
            OrganizationRole.Member);

        Assert.Multiple(() =>
        {
            Assert.That(first.Status, Is.EqualTo(OrganizationInvitationCreationStatus.Created));
            Assert.That(second.Status, Is.EqualTo(OrganizationInvitationCreationStatus.Created));
            Assert.That(
                overCapacity.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.SeatCapacityReached));
        });
    }

    [Test]
    public async Task PaidQuantityTakesPrecedenceOverLargerTransferredTrialCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-trial-capacity", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Trial Capacity",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime,
            SignupTime.AddDays(30));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var service = serviceTest.Service;

        var first = await service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "first@example.com",
            OrganizationRole.Member);
        var overCapacity = await service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "second@example.com",
            OrganizationRole.Member);

        Assert.Multiple(() =>
        {
            Assert.That(first.Status, Is.EqualTo(OrganizationInvitationCreationStatus.Created));
            Assert.That(
                overCapacity.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.SeatCapacityReached));
        });
    }

    [Test]
    public async Task PaidCapacityStartsInclusivelyAndRemainsEligibleWhenCancellationIsScheduled()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-start-capacity", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Start Capacity",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60),
            cancelAtPeriodEnd: true);
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(30));
        var result = await serviceTest.Service
            .CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member);

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationCreationStatus.Created));
    }

    [Test]
    public async Task FuturePaidPeriodDoesNotFundInvitationsAfterTrialEnds()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-future-capacity", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Future Capacity",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime.AddDays(32),
            SignupTime.AddDays(60));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(31));
        var result = await serviceTest.Service
            .CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member);

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationCreationStatus.NoActiveSeatCapacity));
    }

    [Test]
    public async Task InactivePaidProjectionFallsBackToActiveTransferredTrialCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-trial-fallback", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Trial Fallback",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Unpaid,
            seatQuantity: 1,
            SignupTime,
            SignupTime.AddDays(60));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var service = serviceTest.Service;
        var results = new List<OrganizationInvitationCreationResult>();
        for (var index = 0; index < 5; index++)
        {
            results.Add(await service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                $"invitee-{index}@example.com",
                OrganizationRole.Member));
        }

        Assert.Multiple(() =>
        {
            Assert.That(
                results.Take(4).Select(result => result.Status),
                Is.All.EqualTo(OrganizationInvitationCreationStatus.Created));
            Assert.That(
                results[4].Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.SeatCapacityReached));
        });
    }

    [Test]
    public async Task InactiveProjectionCannotRestoreCapacityFromATerminatedTransferredTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "paid-terminated-trial-capacity",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Terminated Trial Capacity",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime,
            SignupTime.AddDays(60),
            projectedAt: SignupTime.AddDays(2));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Canceled,
            seatQuantity: 2,
            SignupTime,
            SignupTime.AddDays(60),
            projectedAt: SignupTime.AddDays(3),
            expectedStatus: CommercialSubscriptionProjectionStatus.Updated);
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(4));
        var result = await serviceTest.Service
            .CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member);

        await using var verificationContext = database.CreateContext();
        var trial = await verificationContext.Trials.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.NoActiveSeatCapacity));
            Assert.That(trial.TerminatedAt, Is.EqualTo(SignupTime.AddDays(2)));
        });
    }

    [TestCase(CommercialSubscriptionStatus.Unpaid, 60)]
    [TestCase(CommercialSubscriptionStatus.Paused, 60)]
    [TestCase(CommercialSubscriptionStatus.Incomplete, 60)]
    [TestCase(CommercialSubscriptionStatus.IncompleteExpired, 60)]
    [TestCase(CommercialSubscriptionStatus.Trialing, 60)]
    [TestCase(CommercialSubscriptionStatus.Canceled, 60)]
    [TestCase(CommercialSubscriptionStatus.Active, 31)]
    public async Task IneligibleOrExpiredPaidSubscriptionDoesNotFundInvitations(
        CommercialSubscriptionStatus status,
        int periodEndsAfterSignupDays)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"inactive-capacity-{status}-{periodEndsAfterSignupDays}",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            $"Inactive Capacity {status} {periodEndsAfterSignupDays}",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            status,
            seatQuantity: 5,
            SignupTime.AddDays(30),
            SignupTime.AddDays(periodEndsAfterSignupDays));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(31));
        var result = await serviceTest.Service
            .CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member);

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationCreationStatus.NoActiveSeatCapacity));
    }

    [Test]
    public async Task PaidSubscriptionAllowsExpiredInvitationToBeResentAfterTrialEnds()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-resend", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Resend",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
            database, SignupTime.AddDays(31));
        var result = await serviceTest.Service
            .ResendAsync(
                owner.UserId,
                organization.OrganizationId,
                invitation.InvitationId);

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationResendStatus.Resent));
        await using var verificationContext = database.CreateContext();
        Assert.That(
            (await verificationContext.OrganizationInvitations.SingleAsync()).ReservedSeatCapacity,
            Is.EqualTo(2));
    }

    [Test]
    public async Task PaidInvitationReservationCanBeAcceptedAfterThePaidPeriodEnds()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-accept-owner", "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "paid-accept-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Acceptance",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime.AddDays(29),
            SignupTime.AddDays(31));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(30));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
            database, SignupTime.AddDays(31));
        var result = await serviceTest.Service
            .AcceptAsync(invitee.UserId, invitation.Secret.Reveal());

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationAcceptanceStatus.Accepted));
    }

    [Test]
    public async Task PaidInvitationReservationCanBeAcceptedAfterPurchasedQuantityDrops()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-drop-owner", "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "paid-drop-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Quantity Drop",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(31));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 1,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60),
            projectedAt: SignupTime.AddDays(32),
            expectedStatus: CommercialSubscriptionProjectionStatus.Updated);
        await using var serviceTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
            database, SignupTime.AddDays(32));
        var result = await serviceTest.Service
            .AcceptAsync(invitee.UserId, invitation.Secret.Reveal());

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationAcceptanceStatus.Accepted));
        await using var verificationContext = database.CreateContext();
        var persistedInvitation = await verificationContext.OrganizationInvitations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persistedInvitation.ReservedSeatCapacity, Is.EqualTo(2));
            Assert.That(
                verificationContext.Seats.Count(seat =>
                    seat.BillingAccountId == organization.BillingAccountId),
                Is.EqualTo(2));
        });
    }

    [Test]
    public async Task CapacityIncreasePromotesEarlierReservationsBeforeLaterAcceptance()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "paid-increase-owner",
            "owner@example.com");
        var earlierInvitee = await SignUpAsync(
            database,
            "paid-increase-earlier",
            "earlier@example.com");
        var laterInvitee = await SignUpAsync(
            database,
            "paid-increase-later",
            "later@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Capacity Increase",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60));
        var earlierInvitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "earlier@example.com",
            SignupTime.AddDays(31));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 10,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60),
            projectedAt: SignupTime.AddDays(32),
            expectedStatus: CommercialSubscriptionProjectionStatus.Updated);
        var laterInvitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "later@example.com",
            SignupTime.AddDays(33));

        OrganizationInvitationAcceptanceResult laterAcceptance;
        await using (var laterTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
                         database, SignupTime.AddDays(34)))
        {
            laterAcceptance = await laterTest.Service.AcceptAsync(
                laterInvitee.UserId, laterInvitation.Secret.Reveal());
        }

        OrganizationInvitationAcceptanceResult earlierAcceptance;
        await using (var earlierTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
                         database, SignupTime.AddDays(35)))
        {
            earlierAcceptance = await earlierTest.Service.AcceptAsync(
                earlierInvitee.UserId, earlierInvitation.Secret.Reveal());
        }

        await using var verificationContext = database.CreateContext();
        var invitations = await verificationContext.OrganizationInvitations
            .OrderBy(invitation => invitation.CreatedAt)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                laterAcceptance.Status,
                Is.EqualTo(OrganizationInvitationAcceptanceStatus.Accepted));
            Assert.That(
                earlierAcceptance.Status,
                Is.EqualTo(OrganizationInvitationAcceptanceStatus.Accepted));
            Assert.That(invitations, Has.Length.EqualTo(2));
            Assert.That(
                invitations.Select(invitation => invitation.ReservedSeatCapacity),
                Is.All.EqualTo(10));
            Assert.That(
                verificationContext.Seats.Count(seat =>
                    seat.BillingAccountId == organization.BillingAccountId),
                Is.EqualTo(3));
        });
    }

    [Test]
    public async Task ActivePaidInvitationCanBeResentAfterSubscriptionBecomesInactive()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-reserved-resend", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Reserved Resend",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(31));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Unpaid,
            seatQuantity: 1,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60),
            projectedAt: SignupTime.AddDays(32),
            expectedStatus: CommercialSubscriptionProjectionStatus.Updated);
        await using var serviceTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
            database, SignupTime.AddDays(32));
        var result = await serviceTest.Service
            .ResendAsync(
                owner.UserId,
                organization.OrganizationId,
                invitation.InvitationId);

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationResendStatus.Resent));
        await using var verificationContext = database.CreateContext();
        Assert.That(
            (await verificationContext.OrganizationInvitations.SingleAsync()).ReservedSeatCapacity,
            Is.EqualTo(2));
    }

    [Test]
    public async Task ConcurrentPaidInvitationsCannotExceedPurchasedQuantity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-race-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "paid-race-admin", "admin@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Race",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin);
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 4,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60));
        await using (var preloadTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
                         database, SignupTime.AddDays(31)))
        {
            var preload = await preloadTest.Service.CreateAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    "preload@example.com",
                    OrganizationRole.Member);
            Assert.That(preload.Status, Is.EqualTo(OrganizationInvitationCreationStatus.Created));
        }
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        await using var ownerTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database,
            SignupTime.AddDays(31),
            interceptors: [new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations")]);
        await using var adminTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database,
            SignupTime.AddDays(31),
            interceptors: [new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations")]);

        var results = await Task.WhenAll(
            ownerTest.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "owner-race@example.com",
                OrganizationRole.Member),
            adminTest.Service.CreateAsync(
                admin.UserId,
                organization.OrganizationId,
                "admin-race@example.com",
                OrganizationRole.Member));

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                results.Count(result =>
                    result.Status == OrganizationInvitationCreationStatus.Created),
                Is.EqualTo(1));
            Assert.That(
                results.Count(result =>
                    result.Status == OrganizationInvitationCreationStatus.SeatCapacityReached),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task InvitationCapacityObservesAnInFlightPurchasedQuantityReduction()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-reduction-race", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Reduction Race",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60));

        var projectionSaveGate = new DatabaseCommandGate();
        var invitationCapacityGate = new DatabaseCommandGate();
        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime.AddDays(31), interceptors: [new SavedChangesGateInterceptor(projectionSaveGate)]);
        var projectionTask = projectionTest.Service.ApplyAsync(
                organization.BillingAccountId,
                new CommercialSubscriptionProjection(
                    "cus_capacity",
                    "sub_capacity",
                    "price_capacity",
                    CommercialSubscriptionStatus.Active,
                    seatQuantity: 1,
                    cancelAtPeriodEnd: false,
                    SignupTime.AddDays(30),
                    SignupTime.AddDays(60),
                    SignupTime.AddDays(31)));

        OrganizationInvitationCreationResult invitationResult;
        CommercialSubscriptionProjectionResult projectionResult;
        try
        {
            await projectionSaveGate.WaitUntilReachedAsync();
            await using var invitationTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
                database,
                SignupTime.AddDays(31),
                interceptors: [new DatabaseCommandGateInterceptor(
                    invitationCapacityGate,
                    "UPDATE billing_accounts")]);
            var invitationTask = invitationTest.Service.CreateAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    "invitee@example.com",
                    OrganizationRole.Member);
            await invitationCapacityGate.WaitUntilReachedAsync();
            invitationCapacityGate.Release();
            projectionSaveGate.Release();

            projectionResult = await projectionTask;
            invitationResult = await invitationTask;
        }
        finally
        {
            invitationCapacityGate.Release();
            projectionSaveGate.Release();
        }

        Assert.Multiple(() =>
        {
            Assert.That(
                projectionResult.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.Updated));
            Assert.That(
                invitationResult.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.SeatCapacityReached));
        });
    }

    [Test]
    public async Task AcceptanceWaitsForInFlightQuantityReductionAndUsesItsReservation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-accept-race-owner", "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "paid-accept-race-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Acceptance Race",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(31));
        var projectionSaveGate = new DatabaseCommandGate();
        var acceptanceBillingSerializationGate = new DatabaseCommandGate();
        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime.AddDays(32), interceptors: [new SavedChangesGateInterceptor(projectionSaveGate)]);
        var projectionTask = projectionTest.Service.ApplyAsync(
                organization.BillingAccountId,
                new CommercialSubscriptionProjection(
                    "cus_capacity",
                    "sub_capacity",
                    "price_capacity",
                    CommercialSubscriptionStatus.Active,
                    seatQuantity: 1,
                    cancelAtPeriodEnd: false,
                    SignupTime.AddDays(30),
                    SignupTime.AddDays(60),
                    SignupTime.AddDays(32)));

        OrganizationInvitationAcceptanceResult acceptanceResult;
        CommercialSubscriptionProjectionResult projectionResult;
        try
        {
            await projectionSaveGate.WaitUntilReachedAsync();
            await using var acceptanceTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
                database,
                SignupTime.AddDays(32),
                interceptors: [new DatabaseCommandGateInterceptor(
                    acceptanceBillingSerializationGate,
                    "UPDATE billing_accounts")]);
            var acceptanceTask = acceptanceTest.Service.AcceptAsync(
                invitee.UserId, invitation.Secret.Reveal());
            await acceptanceBillingSerializationGate.WaitUntilReachedAsync();
            Assert.That(acceptanceTask.IsCompleted, Is.False);
            acceptanceBillingSerializationGate.Release();
            projectionSaveGate.Release();

            projectionResult = await projectionTask;
            acceptanceResult = await acceptanceTask;
        }
        finally
        {
            acceptanceBillingSerializationGate.Release();
            projectionSaveGate.Release();
        }

        await using var verificationContext = database.CreateContext();
        var persistedSubscription = await verificationContext.CommercialSubscriptions.SingleAsync();
        var persistedInvitation = await verificationContext.OrganizationInvitations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                projectionResult.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.Updated));
            Assert.That(persistedSubscription.SeatQuantity, Is.EqualTo(1));
            Assert.That(
                acceptanceResult.Status,
                Is.EqualTo(OrganizationInvitationAcceptanceStatus.Accepted));
            Assert.That(persistedInvitation.ReservedSeatCapacity, Is.EqualTo(2));
            Assert.That(
                verificationContext.OrganizationMemberships.Any(membership =>
                    membership.OrganizationId == organization.OrganizationId
                    && membership.UserId == invitee.UserId),
                Is.True);
            Assert.That(
                verificationContext.Seats.Any(seat =>
                    seat.BillingAccountId == organization.BillingAccountId
                    && seat.AssignedUserId == invitee.UserId),
                Is.True);
        });
    }

    [Test]
    public async Task ExpiredResendObservesInFlightSubscriptionDeactivationWithoutSideEffects()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "paid-resend-race", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Resend Race",
            SignupTime.AddDays(1));
        await ProjectAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(31));
        int auditCountBefore;
        int outboxCountBefore;
        await using (var beforeContext = database.CreateContext())
        {
            auditCountBefore = await beforeContext.AuditRecords.CountAsync();
            outboxCountBefore = await beforeContext.OutboxMessages.CountAsync();
        }

        var projectionSaveGate = new DatabaseCommandGate();
        var resendBillingSerializationGate = new DatabaseCommandGate();
        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database,
            SignupTime.AddDays(38),
            interceptors: [new SavedChangesGateInterceptor(projectionSaveGate)]);
        var projectionTask = projectionTest.Service.ApplyAsync(
                organization.BillingAccountId,
                new CommercialSubscriptionProjection(
                    "cus_capacity",
                    "sub_capacity",
                    "price_capacity",
                    CommercialSubscriptionStatus.Unpaid,
                    seatQuantity: 1,
                    cancelAtPeriodEnd: false,
                    SignupTime.AddDays(30),
                    SignupTime.AddDays(60),
                    SignupTime.AddDays(38)));

        OrganizationInvitationResendResult resendResult;
        try
        {
            await projectionSaveGate.WaitUntilReachedAsync();
            await using var resendTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
                database,
                SignupTime.AddDays(38),
                interceptors: [new DatabaseCommandGateInterceptor(
                    resendBillingSerializationGate,
                    "UPDATE billing_accounts")]);
            var resendTask = resendTest.Service.ResendAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    invitation.InvitationId);
            await resendBillingSerializationGate.WaitUntilReachedAsync();
            resendBillingSerializationGate.Release();
            projectionSaveGate.Release();

            await projectionTask;
            resendResult = await resendTask;
        }
        finally
        {
            resendBillingSerializationGate.Release();
            projectionSaveGate.Release();
        }

        await using var verificationContext = database.CreateContext();
        var persistedInvitation = await verificationContext.OrganizationInvitations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                resendResult.Status,
                Is.EqualTo(OrganizationInvitationResendStatus.NoActiveSeatCapacity));
            Assert.That(persistedInvitation.SecretHash, Is.EqualTo(invitation.Secret.Hash));
            Assert.That(persistedInvitation.LastSentAt, Is.EqualTo(SignupTime.AddDays(31)));
            Assert.That(persistedInvitation.ReservedSeatCapacity, Is.EqualTo(2));
            Assert.That(verificationContext.AuditRecords.Count(), Is.EqualTo(auditCountBefore));
            Assert.That(verificationContext.OutboxMessages.Count(), Is.EqualTo(outboxCountBefore));
        });
    }

    private static async Task ProjectAsync(
        PostgresTestDatabase database,
        Guid billingAccountId,
        CommercialSubscriptionStatus status,
        int seatQuantity,
        DateTimeOffset periodStart,
        DateTimeOffset periodEnd,
        bool cancelAtPeriodEnd = false,
        DateTimeOffset? projectedAt = null,
        CommercialSubscriptionProjectionStatus expectedStatus =
            CommercialSubscriptionProjectionStatus.Created)
    {
        await using var serviceTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, projectedAt ?? periodStart);
        var result = await serviceTest.Service.ApplyAsync(
                billingAccountId,
                new CommercialSubscriptionProjection(
                    "cus_capacity",
                    "sub_capacity",
                    "price_capacity",
                    status,
                    seatQuantity,
                    cancelAtPeriodEnd,
                    periodStart,
                    periodEnd,
                    projectedAt ?? periodStart));
        Assert.That(result.Status, Is.EqualTo(expectedStatus));
    }
}
