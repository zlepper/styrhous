using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Npgsql;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationAcceptancePersistenceTests
{

    [Test]
    public async Task InviteeAcceptsMembershipAndSeatWithAuditsInSameTransaction()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "accept-owner", "owner@example.com");
        var invitee = await SignUpAsync(database, "accept-invitee", "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            " Invitee@Example.com ",
            SignupTime.AddDays(2),
            OrganizationRole.Admin);
        var observedAt = SignupTime.AddDays(3).ToOffset(TimeSpan.FromHours(2));
        await using var context = database.CreateContext();
        await using var serviceFixture1 = CreateAcceptanceService(database, observedAt);
        var service = serviceFixture1.Service;

        var result = await service.AcceptAsync(invitee.UserId, invitation.Secret.Reveal());
        var success = RequireSuccess(result);

        Assert.Multiple(() =>
        {
            Assert.That(success.InvitationId, Is.EqualTo(invitation.InvitationId));
            Assert.That(success.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(success.MembershipId.Version, Is.EqualTo(7));
            Assert.That(success.SeatId.Version, Is.EqualTo(7));
            Assert.That(success.CorrelationId.Version, Is.EqualTo(7));
            Assert.That(success.Role, Is.EqualTo(OrganizationRole.Admin));
            Assert.That(success.AcceptedAt, Is.EqualTo(observedAt.ToUniversalTime()));
            Assert.That(success.AcceptedAt.Offset, Is.EqualTo(TimeSpan.Zero));
            Assert.That(success.ToString(), Does.Not.Contain(invitation.Secret.Reveal()));
        });
        await using var verificationContext = database.CreateContext();
        var persistedInvitation = await verificationContext.OrganizationInvitations.SingleAsync();
        var membership = await verificationContext.OrganizationMemberships.SingleAsync(
            candidate => candidate.UserId == invitee.UserId);
        var seat = await verificationContext.Seats.SingleAsync(
            candidate => candidate.BillingAccountId == organization.BillingAccountId
                && candidate.AssignedUserId == invitee.UserId);
        var audits = await verificationContext.AuditRecords
            .Where(record => record.CorrelationId == success.CorrelationId)
            .OrderBy(record => record.Action)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persistedInvitation.SecretHash, Is.EqualTo(invitation.Secret.Hash));
            Assert.That(persistedInvitation.AcceptedAt, Is.EqualTo(observedAt.ToUniversalTime()));
            Assert.That(persistedInvitation.AcceptedByUserId, Is.EqualTo(invitee.UserId));
            Assert.That(membership.Id, Is.EqualTo(success.MembershipId));
            Assert.That(membership.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(membership.Role, Is.EqualTo(OrganizationRole.Admin));
            Assert.That(membership.CreatedAt, Is.EqualTo(observedAt.ToUniversalTime()));
            Assert.That(seat.Id, Is.EqualTo(success.SeatId));
            Assert.That(seat.BillingAccountId, Is.EqualTo(organization.BillingAccountId));
            Assert.That(seat.AssignedUserId, Is.EqualTo(invitee.UserId));
            Assert.That(seat.DeviceLimit, Is.EqualTo(Seat.DefaultDeviceLimit));
            Assert.That(seat.CreatedAt, Is.EqualTo(observedAt.ToUniversalTime()));
            Assert.That(
                audits.Select(record => record.Action),
                Is.EquivalentTo(new[]
                {
                    AuditAction.OrganizationInvitationAccepted,
                    AuditAction.OrganizationMemberAssigned,
                    AuditAction.SeatAssigned,
                }));
            Assert.That(audits.Select(record => record.ActorUserId),
                Is.All.EqualTo(invitee.UserId));
            Assert.That(audits.Select(record => record.OccurredAt),
                Is.All.EqualTo(observedAt.ToUniversalTime()));
            Assert.That(audits.Select(record => record.Id.Version), Is.All.EqualTo(7));
            Assert.That(
                audits.Single(record =>
                    record.Action == AuditAction.OrganizationInvitationAccepted).TargetId,
                Is.EqualTo(invitation.InvitationId));
            Assert.That(
                audits.Single(record =>
                    record.Action == AuditAction.OrganizationInvitationAccepted).TargetType,
                Is.EqualTo(AuditTargetType.OrganizationInvitation));
            Assert.That(
                audits.Single(record =>
                    record.Action == AuditAction.OrganizationMemberAssigned).TargetId,
                Is.EqualTo(membership.Id));
            Assert.That(
                audits.Single(record =>
                    record.Action == AuditAction.OrganizationMemberAssigned).TargetType,
                Is.EqualTo(AuditTargetType.OrganizationMembership));
            Assert.That(
                audits.Single(record => record.Action == AuditAction.SeatAssigned).TargetId,
                Is.EqualTo(seat.Id));
            Assert.That(
                audits.Single(record => record.Action == AuditAction.SeatAssigned).TargetType,
                Is.EqualTo(AuditTargetType.Seat));
        });
    }

    [Test]
    public async Task UnknownSecretDoesNotRevealOrChangeInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "accept-unknown-owner", "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-unknown-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Unknown Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using var context = database.CreateContext();

        await using var serviceFixture2 = CreateAcceptanceService(database, SignupTime.AddDays(3));
        var result = await serviceFixture2.Service.AcceptAsync(
            invitee.UserId,
            "unknown-invitation-secret");

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationAcceptanceStatus.InvitationNotFound));
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.SecretHash, Is.EqualTo(invitation.Secret.Hash));
            Assert.That(persisted.AcceptedAt, Is.Null);
            Assert.That(
                verificationContext.OrganizationMemberships.Count(
                    membership => membership.OrganizationId == organization.OrganizationId),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task AcceptanceRequiresMatchingVerifiedEmail()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "accept-email-owner", "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-email-invitee",
            "different@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Email Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using var context = database.CreateContext();

        await using var serviceFixture3 = CreateAcceptanceService(database, SignupTime.AddDays(3));
        var result = await serviceFixture3.Service.AcceptAsync(
            invitee.UserId,
            invitation.Secret.Reveal());

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationAcceptanceStatus.EmailMismatch));
        Assert.That((await context.OrganizationInvitations.SingleAsync()).AcceptedAt, Is.Null);
    }

    [Test]
    public async Task HistoricalVerifiedEmailCanAcceptInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-history-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-history-invitee",
            "current@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance History Organization",
            SignupTime.AddDays(1));
        await using (var claimContext = database.CreateContext())
        {
            claimContext.VerifiedEmailClaims.Add(VerifiedEmailClaim.Create(
                invitee.UserId,
                VerifiedExternalIdentity.Create(
                    "github",
                    "historical-email-claim",
                    "historical@example.com"),
                SignupTime.AddDays(1)));
            await claimContext.SaveChangesAsync();
        }

        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "historical@example.com");
        await using var context = database.CreateContext();

        await using var serviceFixture4 = CreateAcceptanceService(database, SignupTime.AddDays(3));
        var result = await serviceFixture4.Service.AcceptAsync(
            invitee.UserId,
            invitation.Secret.Reveal());

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationAcceptanceStatus.Accepted));
    }

    [Test]
    public async Task ExistingMembershipWithoutSeatPreventsAcceptance()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "accept-member-owner", "owner@example.com");
        var invitee = await SignUpAsync(database, "accept-member-invitee", "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Member Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using (var setupContext = database.CreateContext())
        {
            setupContext.OrganizationMemberships.Add(
                OrganizationMembership.AcceptInvitation(
                    organization.OrganizationId,
                    invitee.UserId,
                    OrganizationRole.Member,
                    SignupTime.AddDays(2)));
            await setupContext.SaveChangesAsync();
        }
        await using var context = database.CreateContext();

        await using var serviceFixture5 = CreateAcceptanceService(database, SignupTime.AddDays(3));
        var result = await serviceFixture5.Service.AcceptAsync(
            invitee.UserId,
            invitation.Secret.Reveal());

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationAcceptanceStatus.AlreadyMember));
        await using var verificationContext = database.CreateContext();
        var invitationAcceptedAt = (await verificationContext.OrganizationInvitations.SingleAsync()).AcceptedAt;
        var membershipCount = await verificationContext.OrganizationMemberships.CountAsync(
            membership => membership.OrganizationId == organization.OrganizationId
                && membership.UserId == invitee.UserId);
        var seatCount = await verificationContext.Seats.CountAsync(
            seat => seat.BillingAccountId == organization.BillingAccountId
                && seat.AssignedUserId == invitee.UserId);
        Assert.Multiple(() =>
        {
            Assert.That(invitationAcceptedAt, Is.Null);
            Assert.That(membershipCount, Is.EqualTo(1));
            Assert.That(seatCount, Is.Zero);
        });
    }

    [Test]
    public async Task ExistingSeatWithoutMembershipPreventsAcceptance()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-seat-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-seat-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Seat Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using (var setupContext = database.CreateContext())
        {
            setupContext.Seats.Add(Seat.Assign(
                organization.BillingAccountId,
                invitee.UserId,
                SignupTime.AddDays(2)));
            await setupContext.SaveChangesAsync();
        }

        await using var context = database.CreateContext();
        await using var serviceFixture6 = CreateAcceptanceService(database, SignupTime.AddDays(3));
        var result = await serviceFixture6.Service.AcceptAsync(
            invitee.UserId,
            invitation.Secret.Reveal());

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationAcceptanceStatus.AlreadyMember));
        await using var verificationContext = database.CreateContext();
        var invitationAcceptedAt = (await verificationContext.OrganizationInvitations.SingleAsync()).AcceptedAt;
        var membershipCount = await verificationContext.OrganizationMemberships.CountAsync(
            membership => membership.OrganizationId == organization.OrganizationId
                && membership.UserId == invitee.UserId);
        var seatCount = await verificationContext.Seats.CountAsync(
            seat => seat.BillingAccountId == organization.BillingAccountId
                && seat.AssignedUserId == invitee.UserId);
        Assert.Multiple(() =>
        {
            Assert.That(invitationAcceptedAt, Is.Null);
            Assert.That(membershipCount, Is.Zero);
            Assert.That(seatCount, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ExpiredCancelledAndAcceptedInvitationsShareNotFoundStatus()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "accept-terminal-owner", "owner@example.com");
        var actor = await SignUpAsync(database, "accept-terminal-actor", "actor@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Terminal Organization",
            SignupTime.AddDays(1));
        var expired = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "expired@example.com",
            SignupTime.AddDays(2));
        var cancelled = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "cancelled@example.com",
            SignupTime.AddDays(2));
        var accepted = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "accepted@example.com",
            SignupTime.AddDays(2));
        await using (var preparationContext = database.CreateContext())
        {
            await preparationContext.OrganizationInvitations
                .Where(invitation => invitation.Id == cancelled.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    invitation => invitation.CancelledAt,
                    SignupTime.AddDays(3)));
            await preparationContext.OrganizationInvitations
                .Where(invitation => invitation.Id == accepted.InvitationId)
                .ExecuteUpdateAsync(setters => setters
                    .SetProperty(invitation => invitation.AcceptedAt, SignupTime.AddDays(3))
                    .SetProperty(invitation => invitation.AcceptedByUserId, actor.UserId));
        }

        await using var context = database.CreateContext();
        await using var serviceFixture7 = CreateAcceptanceService(database, expired.ExpiresAt);
        var service = serviceFixture7.Service;
        var results = new[]
        {
            await service.AcceptAsync(actor.UserId, expired.Secret.Reveal()),
            await service.AcceptAsync(actor.UserId, cancelled.Secret.Reveal()),
            await service.AcceptAsync(actor.UserId, accepted.Secret.Reveal()),
        };

        Assert.That(
            results.Select(result => result.Status),
            Is.All.EqualTo(OrganizationInvitationAcceptanceStatus.InvitationNotFound));
    }

    [Test]
    public async Task DatabaseRejectsAcceptanceAtInvitationExpiry()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "acceptance-expiry-owner",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Expiry Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using var context = database.CreateContext();

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(candidate => candidate.Id == invitation.InvitationId)
                .ExecuteUpdateAsync(setters => setters
                    .SetProperty(candidate => candidate.AcceptedAt, invitation.ExpiresAt)
                    .SetProperty(candidate => candidate.AcceptedByUserId, owner.UserId)));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_organization_invitations_terminal_state"));
    }

    [Test]
    public async Task AcceptanceAfterTrialExpiryUsesPreviouslyReservedCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "accept-expiry-owner", "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-expiry-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Expired Trial Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(29));
        for (var index = 0; index < 3; index++)
        {
            await CreateInvitationAsync(
                database,
                owner.UserId,
                organization.OrganizationId,
                $"pending-{index}@example.com",
                SignupTime.AddDays(29));
        }

        await using var context = database.CreateContext();

        await using var serviceFixture8 = CreateAcceptanceService(database, SignupTime.AddDays(31));
        var result = await serviceFixture8.Service.AcceptAsync(
            invitee.UserId,
            invitation.Secret.Reveal());

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationAcceptanceStatus.Accepted));
        var assignedSeatCount = await context.Seats.CountAsync(
            seat => seat.BillingAccountId == organization.BillingAccountId);
        var activeInvitationCount = await context.OrganizationInvitations.CountAsync(
            candidate => candidate.OrganizationId == organization.OrganizationId
                && candidate.AcceptedAt == null
                && candidate.CancelledAt == null
                && candidate.ExpiresAt > SignupTime.AddDays(31));
        Assert.Multiple(() =>
        {
            Assert.That(assignedSeatCount, Is.EqualTo(2));
            Assert.That(activeInvitationCount, Is.EqualTo(3));
            Assert.That(assignedSeatCount + activeInvitationCount, Is.EqualTo(5));
        });
    }

    [Test]
    public async Task AcceptanceCannotUseCapacityReallocatedAfterInvitationExpiry()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-expiry-race-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-expiry-race-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Expiry Race Organization",
            SignupTime.AddDays(1));
        var expiring = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        for (var index = 0; index < 3; index++)
        {
            await CreateInvitationAsync(
                database,
                owner.UserId,
                organization.OrganizationId,
                $"reserved-{index}@example.com",
                SignupTime.AddDays(3));
        }

        var acceptanceGate = new DatabaseCommandGate();
        await using var serviceFixture9 = CreateAcceptanceService(
                database,
                expiring.ExpiresAt.AddTicks(-1),
                new DatabaseCommandGateInterceptor(acceptanceGate, "UPDATE organizations"));
        var acceptanceTask = serviceFixture9.Service
            .AcceptAsync(invitee.UserId, expiring.Secret.Reveal());
        await acceptanceGate.WaitUntilReachedAsync();
        var replacement = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "replacement@example.com",
            expiring.ExpiresAt);
        acceptanceGate.Release();

        var acceptanceResult = await acceptanceTask;

        Assert.That(replacement.InvitationId.Version, Is.EqualTo(7));
        Assert.That(
            acceptanceResult.Status,
            Is.EqualTo(OrganizationInvitationAcceptanceStatus.InvitationNotFound));
        await using var verificationContext = database.CreateContext();
        var assignedSeatCount = await verificationContext.Seats.CountAsync(
            seat => seat.BillingAccountId == organization.BillingAccountId);
        var activeInvitationCount = await verificationContext.OrganizationInvitations.CountAsync(
            invitation => invitation.OrganizationId == organization.OrganizationId
                && invitation.AcceptedAt == null
                && invitation.CancelledAt == null
                && invitation.ExpiresAt > expiring.ExpiresAt);
        var membershipCount = await verificationContext.OrganizationMemberships.CountAsync(
            membership => membership.OrganizationId == organization.OrganizationId
                && membership.UserId == invitee.UserId);
        Assert.Multiple(() =>
        {
            Assert.That(assignedSeatCount, Is.EqualTo(1));
            Assert.That(activeInvitationCount, Is.EqualTo(4));
            Assert.That(assignedSeatCount + activeInvitationCount, Is.EqualTo(5));
            Assert.That(membershipCount, Is.Zero);
        });
    }

    [Test]
    public async Task ConcurrentAcceptanceCreatesMembershipAndSeatExactlyOnce()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "accept-race-owner", "owner@example.com");
        var invitee = await SignUpAsync(database, "accept-race-invitee", "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Race Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        await using var serviceFixture10 = CreateAcceptanceService(
                database,
                SignupTime.AddDays(3),
                new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));
        await using var serviceFixture11 = CreateAcceptanceService(
                database,
                SignupTime.AddDays(3),
                new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));
        var results = await Task.WhenAll(
            serviceFixture10.Service.AcceptAsync(
                invitee.UserId,
                invitation.Secret.Reveal()),
            serviceFixture11.Service.AcceptAsync(
                invitee.UserId,
                invitation.Secret.Reveal()));

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                results.Count(result =>
                    result.Status == OrganizationInvitationAcceptanceStatus.Accepted),
                Is.EqualTo(1));
            Assert.That(
                results.Count(result =>
                    result.Status == OrganizationInvitationAcceptanceStatus.InvitationNotFound),
                Is.EqualTo(1));
        });
        await using var verificationContext = database.CreateContext();
        var membershipCount = await verificationContext.OrganizationMemberships.CountAsync(
            membership => membership.OrganizationId == organization.OrganizationId
                && membership.UserId == invitee.UserId);
        var seatCount = await verificationContext.Seats.CountAsync(
            seat => seat.BillingAccountId == organization.BillingAccountId
                && seat.AssignedUserId == invitee.UserId);
        var acceptanceAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.OrganizationInvitationAccepted);
        Assert.Multiple(() =>
        {
            Assert.That(membershipCount, Is.EqualTo(1));
            Assert.That(seatCount, Is.EqualTo(1));
            Assert.That(acceptanceAuditCount, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ConcurrentAcceptanceAndCancellationHaveExactlyOneWinner()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-cancel-race-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-cancel-race-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Cancellation Race Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        await using var serviceFixture12 = CreateAcceptanceService(
                database,
                SignupTime.AddDays(3),
                new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations"));
        var acceptanceTask = serviceFixture12.Service
            .AcceptAsync(invitee.UserId, invitation.Secret.Reveal());
        await using var cancellationTest = ServiceTestBase<OrganizationInvitationCancellationService>.ForDatabase(
            database,
            SignupTime.AddDays(3),
            interceptors: [new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations")]);
        var cancellationTask = cancellationTest.Service.CancelAsync(
                owner.UserId,
                organization.OrganizationId,
                invitation.InvitationId);
        await Task.WhenAll(acceptanceTask, cancellationTask);
        var acceptanceResult = await acceptanceTask;
        var cancellationResult = await cancellationTask;
        var acceptanceWon = acceptanceResult.Status
            == OrganizationInvitationAcceptanceStatus.Accepted;
        var cancellationWon = cancellationResult.Status
            == OrganizationInvitationCancellationStatus.Cancelled;

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(acceptanceWon, Is.Not.EqualTo(cancellationWon));
            Assert.That(
                acceptanceResult.Status,
                Is.EqualTo(
                    acceptanceWon
                        ? OrganizationInvitationAcceptanceStatus.Accepted
                        : OrganizationInvitationAcceptanceStatus.InvitationNotFound));
            Assert.That(
                cancellationResult.Status,
                Is.EqualTo(
                    cancellationWon
                        ? OrganizationInvitationCancellationStatus.Cancelled
                        : OrganizationInvitationCancellationStatus.InvitationNotFound));
        });
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        var persistedOutboxMessage = await verificationContext.OutboxMessages.SingleAsync();
        var membershipCount = await verificationContext.OrganizationMemberships.CountAsync(
            membership => membership.OrganizationId == organization.OrganizationId
                && membership.UserId == invitee.UserId);
        var seatCount = await verificationContext.Seats.CountAsync(
            seat => seat.BillingAccountId == organization.BillingAccountId
                && seat.AssignedUserId == invitee.UserId);
        var acceptanceAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.OrganizationInvitationAccepted);
        var memberAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.OrganizationMemberAssigned);
        var seatAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.SeatAssigned
                && record.OccurredAt == SignupTime.AddDays(3));
        var cancellationAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.OrganizationInvitationCancelled);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.AcceptedAt is not null, Is.EqualTo(acceptanceWon));
            Assert.That(persisted.CancelledAt is not null, Is.EqualTo(cancellationWon));
            Assert.That(persistedOutboxMessage.DiscardedAt, Is.Not.Null);
            Assert.That(
                persistedOutboxMessage.DiscardReason,
                Is.EqualTo(
                    acceptanceWon
                        ? OutboxDiscardReason.InvitationAccepted
                        : OutboxDiscardReason.InvitationCancelled));
            Assert.That(membershipCount, Is.EqualTo(acceptanceWon ? 1 : 0));
            Assert.That(seatCount, Is.EqualTo(acceptanceWon ? 1 : 0));
            Assert.That(acceptanceAuditCount, Is.EqualTo(acceptanceWon ? 1 : 0));
            Assert.That(memberAuditCount, Is.EqualTo(acceptanceWon ? 1 : 0));
            Assert.That(seatAuditCount, Is.EqualTo(acceptanceWon ? 1 : 0));
            Assert.That(cancellationAuditCount, Is.EqualTo(cancellationWon ? 1 : 0));
        });
    }

    [Test]
    public async Task ResendInvalidatesAcceptanceThatResolvedThePreviousSecret()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "accept-resend-owner",
            "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-resend-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Resend Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var acceptanceGate = new DatabaseCommandGate();
        await using var resendContext = database.CreateContext();

        await using var serviceFixture13 = CreateAcceptanceService(
                database,
                SignupTime.AddDays(3),
                new DatabaseCommandGateInterceptor(acceptanceGate, "UPDATE organizations"));
        var acceptanceTask = serviceFixture13.Service
            .AcceptAsync(invitee.UserId, invitation.Secret.Reveal());
        await acceptanceGate.WaitUntilReachedAsync();
        await using var resendTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
            database, SignupTime.AddDays(4));
        var resendResult = await resendTest.Service.ResendAsync(
                owner.UserId,
                organization.OrganizationId,
                invitation.InvitationId);
        acceptanceGate.Release();
        var acceptanceResult = await acceptanceTask;

        Assert.Multiple(() =>
        {
            Assert.That(
                resendResult.Status,
                Is.EqualTo(OrganizationInvitationResendStatus.Resent));
            Assert.That(
                acceptanceResult.Status,
                Is.EqualTo(OrganizationInvitationAcceptanceStatus.InvitationNotFound));
        });
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        var acceptanceAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.OrganizationInvitationAccepted);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.SecretHash, Is.Not.EqualTo(invitation.Secret.Hash));
            Assert.That(persisted.AcceptedAt, Is.Null);
            Assert.That(acceptanceAuditCount, Is.Zero);
        });
    }

    [Test]
    public async Task FailureAfterSavingRollsBackAcceptanceMembershipSeatAndAudits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "accept-rollback-owner", "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "accept-rollback-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Rollback Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var serviceFixture14 = CreateAcceptanceService(database, SignupTime.AddDays(3), new ThrowAfterSaveInterceptor());
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await serviceFixture14.Service.AcceptAsync(
                invitee.UserId,
                invitation.Secret.Reveal()));

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        var persistedOutboxMessage = await verificationContext.OutboxMessages.SingleAsync();
        var membershipCount = await verificationContext.OrganizationMemberships.CountAsync(
            membership => membership.OrganizationId == organization.OrganizationId);
        var seatCount = await verificationContext.Seats.CountAsync(
            seat => seat.BillingAccountId == organization.BillingAccountId);
        var auditCount = await verificationContext.AuditRecords.CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.AcceptedAt, Is.Null);
            Assert.That(persistedOutboxMessage.DiscardedAt, Is.Null);
            Assert.That(persistedOutboxMessage.DiscardReason, Is.Null);
            Assert.That(membershipCount, Is.EqualTo(1));
            Assert.That(seatCount, Is.EqualTo(1));
            Assert.That(auditCount, Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task UnknownActorCannotAcceptInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();

        await using var serviceFixture15 = CreateAcceptanceService(database, SignupTime);
        Assert.ThrowsAsync<UserNotFoundException>(
            async () => await serviceFixture15.Service.AcceptAsync(
                Guid.CreateVersion7(),
                "unknown-secret"));
        Assert.That(await context.OrganizationInvitations.CountAsync(), Is.Zero);
        Assert.That(await context.OrganizationMemberships.CountAsync(), Is.Zero);
        Assert.That(await context.Seats.CountAsync(), Is.Zero);
        Assert.That(await context.OutboxMessages.CountAsync(), Is.Zero);
        Assert.That(await context.AuditRecords.CountAsync(), Is.Zero);
    }

    [Test]
    public async Task AcceptanceRejectsOversizedSecretBeforePersistence()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var serviceTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
            database, SignupTime);

        Assert.ThrowsAsync<ArgumentException>(
            async () => await serviceTest.Service.AcceptAsync(
                Guid.CreateVersion7(),
                new string('a', OrganizationInvitationSecret.MaximumLength + 1)));
    }

    private static ServiceTestBase<OrganizationInvitationAcceptanceService> CreateAcceptanceService(
        PostgresTestDatabase database,
        DateTimeOffset observedAt,
        params IInterceptor[] interceptors)
    {
        var serviceTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
            database, observedAt, interceptors: interceptors);
        return serviceTest;
    }

    private static OrganizationInvitationAcceptanceResult.Success RequireSuccess(
        OrganizationInvitationAcceptanceResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationInvitationAcceptanceResult.Success>());
        return (OrganizationInvitationAcceptanceResult.Success)result;
    }

}
