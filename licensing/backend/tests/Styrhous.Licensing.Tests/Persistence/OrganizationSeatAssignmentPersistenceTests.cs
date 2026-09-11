using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationSeatAssignmentPersistenceTests
{
    [Test]
    public async Task DisablingAndRestoringSeatKeepsIdentityAndDevices()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "seat-owner", "owner@example.com");
        var member = await SignUpAsync(database, "seat-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat Organization",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var activation = await ActivateAsync(
            database,
            member.UserId,
            memberSetup.SeatId,
            installationNumber: 1,
            SignupTime.AddDays(2));
        Assert.That(activation.ActivationId, Is.Not.Null);

        OrganizationSeatAssignmentResult disabled;
        await using (var assignmentTest = ServiceTestBase<OrganizationSeatAssignmentService>.ForDatabase(
                database, SignupTime.AddDays(3)))
        {
            disabled = await assignmentTest.Service.SetAssignedAsync(
                owner.UserId,
                organization.OrganizationId,
                memberSetup.MembershipId.ToString(),
                assigned: false);
        }

        var disabledSuccess = RequireSuccess(disabled);
        Assert.Multiple(() =>
        {
            Assert.That(disabled.Status, Is.EqualTo(OrganizationSeatAssignmentStatus.Unassigned));
            Assert.That(disabledSuccess.SeatId, Is.EqualTo(memberSetup.SeatId));
            Assert.That(disabledSuccess.Assigned, Is.False);
        });
        await using (var context = database.CreateContext())
        {
            var seat = await context.Seats.SingleAsync(item => item.Id == memberSetup.SeatId);
            Assert.Multiple(() =>
            {
                Assert.That(seat.ProductAccessEnabled, Is.False);
                Assert.That(
                    context.DeviceActivations.Count(item => item.SeatId == memberSetup.SeatId),
                    Is.EqualTo(1));
                Assert.That(
                    context.AuditRecords.Count(item =>
                        item.CorrelationId == disabledSuccess.CorrelationId
                        && item.Action == AuditAction.SeatUnassigned),
                    Is.EqualTo(1));
            });

            await using var listingTest = ServiceTestBase<BillingAccountListingService>.ForDatabase(
                database, SignupTime.AddDays(3));
            var billingAccount = (await listingTest.Service
                    .ListForUserAsync(owner.UserId))
                .Single(item => item.BillingAccountId == organization.BillingAccountId);
            Assert.That(billingAccount.AssignedSeatCount, Is.EqualTo(1));

            await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
                database, SignupTime.AddDays(3));
            var check = await checkTest.Service
                .CheckAsync(member.UserId, activation.ActivationId!.Value);
            Assert.Multiple(() =>
            {
                Assert.That(check.Status, Is.EqualTo(DeviceEntitlementCheckStatus.Ineligible));
                Assert.That(check.ReasonCode, Is.EqualTo(EntitlementReasonCodes.ProductSeatNotAssigned));
            });
        }

        OrganizationSeatAssignmentResult restored;
        await using (var assignmentTest2 = ServiceTestBase<OrganizationSeatAssignmentService>.ForDatabase(
                database, SignupTime.AddDays(4)))
        {
            restored = await assignmentTest2.Service.SetAssignedAsync(
                owner.UserId,
                organization.OrganizationId,
                memberSetup.MembershipId.ToString(),
                assigned: true);
        }

        var restoredSuccess = RequireSuccess(restored);
        await using (var context = database.CreateContext())
        {
            var seat = await context.Seats.SingleAsync(item => item.Id == memberSetup.SeatId);
            Assert.Multiple(() =>
            {
                Assert.That(restored.Status, Is.EqualTo(OrganizationSeatAssignmentStatus.Assigned));
                Assert.That(restoredSuccess.SeatId, Is.EqualTo(memberSetup.SeatId));
                Assert.That(seat.ProductAccessEnabled, Is.True);
                Assert.That(
                    context.DeviceActivations.Count(item => item.SeatId == memberSetup.SeatId),
                    Is.EqualTo(1));
            });

            await using var listingTest2 = ServiceTestBase<BillingAccountListingService>.ForDatabase(
                database, SignupTime.AddDays(4));
            var billingAccount = (await listingTest2.Service
                    .ListForUserAsync(owner.UserId))
                .Single(item => item.BillingAccountId == organization.BillingAccountId);
            Assert.That(billingAccount.AssignedSeatCount, Is.EqualTo(2));
        }
    }

    [Test]
    public async Task SeatlessInvitationCanBeAcceptedWithoutLicensedCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "seatless-owner", "owner@example.com");
        var invitee = await SignUpAsync(database, "seatless-invitee", "invitee@example.com");
        await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Trial Holder",
            SignupTime.AddDays(1));
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seatless Organization",
            SignupTime.AddDays(2));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(3),
            OrganizationRole.Admin,
            assignProductSeat: false);

        OrganizationInvitationAcceptanceResult result;
        await using (var acceptanceTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
                database, SignupTime.AddDays(4)))
        {
            result = await acceptanceTest.Service
                .AcceptAsync(invitee.UserId, invitation.Secret.Reveal());
        }

        Assert.That(result, Is.TypeOf<OrganizationInvitationAcceptanceResult.Success>());
        var success = (OrganizationInvitationAcceptanceResult.Success)result;
        await using var verificationContext = database.CreateContext();
        var persistedInvitation = await verificationContext.OrganizationInvitations
            .SingleAsync(item => item.Id == invitation.InvitationId);
        var seat = await verificationContext.Seats
            .SingleAsync(item => item.Id == success.SeatId);
        Assert.Multiple(() =>
        {
            Assert.That(success.ProductSeatAssigned, Is.False);
            Assert.That(persistedInvitation.AssignProductSeat, Is.False);
            Assert.That(persistedInvitation.ReservedSeatCapacity, Is.Zero);
            Assert.That(seat.ProductAccessEnabled, Is.False);
            Assert.That(
                verificationContext.AuditRecords.Count(item =>
                    item.CorrelationId == success.CorrelationId
                    && item.Action == AuditAction.SeatAssigned),
                Is.Zero);
        });

        await using var listingTest = ServiceTestBase<BillingAccountListingService>.ForDatabase(
            database, SignupTime.AddDays(4));
        var billingAccount = (await listingTest.Service
                .ListForUserAsync(owner.UserId))
            .Single(item => item.BillingAccountId == organization.BillingAccountId);
        Assert.That(billingAccount.AssignedSeatCount, Is.EqualTo(1));
    }

    [Test]
    public async Task EnablingSeatWithoutActiveCapacityIsRejected()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "capacity-owner", "owner@example.com");
        var member = await SignUpAsync(database, "capacity-member", "member@example.com");
        await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Trial Holder",
            SignupTime.AddDays(1));
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "No Capacity",
            SignupTime.AddDays(2));
        var setup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            productAccessEnabled: false);
        await using var context = database.CreateContext();

        await using var assignmentTest = ServiceTestBase<OrganizationSeatAssignmentService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var result = await assignmentTest.Service.SetAssignedAsync(
            owner.UserId,
            organization.OrganizationId,
            setup.MembershipId.ToString(),
            assigned: true);

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationSeatAssignmentStatus.NoActiveSeatCapacity));
        Assert.That(
            (await context.Seats.SingleAsync(item => item.Id == setup.SeatId))
                .ProductAccessEnabled,
            Is.False);
    }

    [Test]
    public async Task ConcurrentMemberRemovalCannotLeaveSeatAssignmentWithStaleMembership()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-removal-race-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "seat-removal-race-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat Removal Race",
            SignupTime.AddDays(1));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        var targetUserClaimGate = new DatabaseCommandGate();

        await using var assignmentTest = ServiceTestBase<OrganizationSeatAssignmentService>.ForDatabase(
            database, SignupTime.AddDays(2), interceptors: [
            new DatabaseCommandGateInterceptor(
                targetUserClaimGate,
                "UPDATE user_accounts",
                matchingOccurrence: 2)]);
        var assignmentTask = assignmentTest.Service
            .SetAssignedAsync(
                owner.UserId,
                organization.OrganizationId,
                memberSetup.MembershipId.ToString(),
                assigned: false);
        await targetUserClaimGate.WaitUntilReachedAsync();
        OrganizationMemberRemovalResult removal;
        try
        {
            await using var removalTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
                database, SignupTime.AddDays(2));
            removal = await removalTest.Service
                .RemoveAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    memberSetup.MembershipId.ToString());
        }
        finally
        {
            targetUserClaimGate.Release();
        }
        var assignment = await assignmentTask;

        Assert.Multiple(() =>
        {
            Assert.That(removal.Status, Is.EqualTo(OrganizationMemberRemovalStatus.Removed));
            Assert.That(
                assignment.Status,
                Is.EqualTo(OrganizationSeatAssignmentStatus.MemberNotFound));
        });
        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.OrganizationMemberships.Any(candidate =>
                    candidate.Id == memberSetup.MembershipId),
                Is.False);
            Assert.That(
                verificationContext.Seats.Any(candidate => candidate.Id == memberSetup.SeatId),
                Is.False);
            Assert.That(
                verificationContext.AuditRecords.Count(record =>
                    record.Action == AuditAction.SeatUnassigned
                    && record.TargetId == memberSetup.SeatId),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ConcurrentAssignmentsCannotOverbookLastTransferredTrialSeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "seat-race-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "seat-race-admin", "admin@example.com");
        var activeOne = await SignUpAsync(database, "seat-race-active-one", "one@example.com");
        var activeTwo = await SignUpAsync(database, "seat-race-active-two", "two@example.com");
        var firstTarget = await SignUpAsync(database, "seat-race-first", "first@example.com");
        var secondTarget = await SignUpAsync(database, "seat-race-second", "second@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat Race Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin);
        await AddOrganizationMemberAsync(
            database,
            organization,
            activeOne.UserId,
            OrganizationRole.Member);
        await AddOrganizationMemberAsync(
            database,
            organization,
            activeTwo.UserId,
            OrganizationRole.Member);
        var firstSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            firstTarget.UserId,
            OrganizationRole.Member,
            productAccessEnabled: false);
        var secondSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            secondTarget.UserId,
            OrganizationRole.Member,
            productAccessEnabled: false);

        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        await using var assignmentTest = ServiceTestBase<OrganizationSeatAssignmentService>.ForDatabase(
            database, SignupTime.AddDays(2), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations")]);
        await using var assignmentTest2 = ServiceTestBase<OrganizationSeatAssignmentService>.ForDatabase(
            database, SignupTime.AddDays(2), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations")]);
        var results = await Task.WhenAll(
            assignmentTest.Service.SetAssignedAsync(
                owner.UserId,
                organization.OrganizationId,
                firstSetup.MembershipId.ToString(),
                assigned: true),
            assignmentTest2.Service.SetAssignedAsync(
                admin.UserId,
                organization.OrganizationId,
                secondSetup.MembershipId.ToString(),
                assigned: true));

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                results.Count(result => result.Status == OrganizationSeatAssignmentStatus.Assigned),
                Is.EqualTo(1));
            Assert.That(
                results.Count(result =>
                    result.Status == OrganizationSeatAssignmentStatus.SeatCapacityReached),
                Is.EqualTo(1));
        });
        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                verificationContext.Seats.Count(seat =>
                    seat.BillingAccountId == organization.BillingAccountId
                    && seat.ProductAccessEnabled),
                Is.EqualTo(5));
            Assert.That(
                verificationContext.AuditRecords.Count(record =>
                    record.Action == AuditAction.SeatAssigned
                    && results.OfType<OrganizationSeatAssignmentResult.Success>()
                        .Select(result => result.CorrelationId)
                        .Contains(record.CorrelationId)),
                Is.EqualTo(1));
        });
    }

    private static OrganizationSeatAssignmentResult.Success RequireSuccess(
        OrganizationSeatAssignmentResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationSeatAssignmentResult.Success>());
        return (OrganizationSeatAssignmentResult.Success)result;
    }
}
