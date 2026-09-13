using Styrhous.Licensing.Infrastructure.Organizations;
using System.Globalization;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Npgsql;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationPersistenceTests
{

    [Test]
    public async Task OwnerCreatesAuditedSevenDayInvitationWithoutPersistingRawSecret()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Organization",
            SignupTime.AddDays(1));
        var observedAt = SignupTime.AddDays(2);
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, observedAt);
        var service = serviceTest.Service;

        var result = await service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "  Invitee@Example.com  ",
            OrganizationRole.Member);
        var success = RequireSuccess(result);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(OrganizationInvitationCreationStatus.Created));
            Assert.That(success.InvitationId.Version, Is.EqualTo(7));
            Assert.That(success.CorrelationId.Version, Is.EqualTo(7));
            Assert.That(success.Secret.Reveal(),
                Is.Not.Empty);
            Assert.That(result.ToString(),
                Does.Not.Contain(success.Secret.Reveal()));
        });
        await using var verificationContext = database.CreateContext();
        var invitation = await verificationContext.OrganizationInvitations.SingleAsync();
        var audit = await verificationContext.AuditRecords.SingleAsync(
            record => record.CorrelationId == success.CorrelationId);
        Assert.Multiple(() =>
        {
            Assert.That(invitation.Id, Is.EqualTo(success.InvitationId));
            Assert.That(invitation.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(invitation.CreatedByUserId, Is.EqualTo(owner.UserId));
            Assert.That(invitation.Email, Is.EqualTo("Invitee@Example.com"));
            Assert.That(invitation.NormalizedEmail, Is.EqualTo("INVITEE@EXAMPLE.COM"));
            Assert.That(invitation.Role, Is.EqualTo(OrganizationRole.Member));
            Assert.That(invitation.SecretHash, Is.EqualTo(success.Secret.Hash));
            Assert.That(invitation.SecretHash, Is.Not.EqualTo(success.Secret.Reveal()));
            Assert.That(invitation.CreatedAt, Is.EqualTo(observedAt));
            Assert.That(invitation.LastSentAt, Is.EqualTo(observedAt));
            Assert.That(invitation.ExpiresAt, Is.EqualTo(observedAt.AddDays(7)));
            Assert.That(invitation.ReservedSeatCapacity, Is.EqualTo(5));
            Assert.That(audit.Action, Is.EqualTo(AuditAction.OrganizationInvitationCreated));
            Assert.That(audit.TargetType, Is.EqualTo(AuditTargetType.OrganizationInvitation));
            Assert.That(audit.TargetId, Is.EqualTo(invitation.Id));
            Assert.That(audit.ActorUserId, Is.EqualTo(owner.UserId));
            Assert.That(audit.OccurredAt, Is.EqualTo(observedAt));
        });
    }

    [TestCase(OrganizationRole.Owner, OrganizationInvitationCreationStatus.Created)]
    [TestCase(OrganizationRole.Admin, OrganizationInvitationCreationStatus.Created)]
    [TestCase(
        OrganizationRole.Member,
        OrganizationInvitationCreationStatus.InsufficientPermission)]
    public async Task OnlyOwnersAndAdminsCanCreateInvitations(
        OrganizationRole actorRole,
        OrganizationInvitationCreationStatus expectedStatus)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "permission-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Permission Organization",
            SignupTime.AddDays(1));
        var actorUserId = owner.UserId;
        if (actorRole != OrganizationRole.Owner)
        {
            var actor = await SignUpAsync(
                database,
                $"permission-{actorRole}",
                $"{actorRole.ToString().ToLowerInvariant()}@example.com");
            actorUserId = actor.UserId;
            await AddOrganizationMemberAsync(database, organization, actorUserId, actorRole);
        }

        await using var context = database.CreateContext();
        await using var serviceFixture1 = CreateService(database, SignupTime.AddDays(2));
        var result = await serviceFixture1.Service
            .CreateAsync(
                actorUserId,
                organization.OrganizationId,
                $"invite-{actorRole.ToString().ToLowerInvariant()}@example.com",
                OrganizationRole.Member);

        Assert.That(result.Status, Is.EqualTo(expectedStatus));
        await using var verificationContext = database.CreateContext();
        var invitationCount = await verificationContext.OrganizationInvitations.CountAsync();
        var invitationAuditCount = await verificationContext.AuditRecords.CountAsync(
            audit => audit.Action == AuditAction.OrganizationInvitationCreated);
        Assert.Multiple(() =>
        {
            Assert.That(invitationCount,
                Is.EqualTo(expectedStatus == OrganizationInvitationCreationStatus.Created ? 1 : 0));
            Assert.That(
                invitationAuditCount,
                Is.EqualTo(expectedStatus == OrganizationInvitationCreationStatus.Created ? 1 : 0));
        });
    }

    [Test]
    public async Task UnknownPersistedRoleDoesNotGainInvitationPermission()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "unknown-role-owner", "owner@example.com");
        var actor = await SignUpAsync(database, "unknown-role-actor", "actor@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Unknown Role Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            actor.UserId,
            (OrganizationRole)999);
        await using var context = database.CreateContext();

        await using var serviceFixture2 = CreateService(database, SignupTime.AddDays(2));
        var result = await serviceFixture2.Service.CreateAsync(
            actor.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            OrganizationRole.Member);

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationCreationStatus.InsufficientPermission));
    }

    [Test]
    public async Task NonMemberCannotDiscoverOrganizationThroughInvitationCreation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "privacy-owner", "owner@example.com");
        var outsider = await SignUpAsync(database, "privacy-outsider", "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Private Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();

        await using var serviceFixture3 = CreateService(database, SignupTime.AddDays(2));
        var result = await serviceFixture3.Service
            .CreateAsync(
                outsider.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member);

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationCreationStatus.OrganizationNotFound));
    }

    [Test]
    public async Task ExistingMemberCannotBeInvitedAgainByVerifiedEmail()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "member-owner", "owner@example.com");
        var member = await SignUpAsync(database, "existing-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Member Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await using var context = database.CreateContext();

        await using var serviceFixture4 = CreateService(database, SignupTime.AddDays(2));
        var result = await serviceFixture4.Service
            .CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "  MEMBER@example.com ",
                OrganizationRole.Admin);

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationCreationStatus.AlreadyMember));
    }

    [Test]
    public async Task PendingInvitationReservesOneSeatAndDeduplicatesNormalizedEmail()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "duplicate-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Duplicate Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture5 = CreateService(database, SignupTime.AddDays(2));
        var service = serviceFixture5.Service;

        var first = await service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "Invitee@Example.com",
            OrganizationRole.Member);
        var duplicate = await service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            " invitee@example.com ",
            OrganizationRole.Admin);
        var invitationCount = await context.OrganizationInvitations.CountAsync();

        Assert.Multiple(() =>
        {
            Assert.That(first.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.Created));
            Assert.That(duplicate.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.InvitationAlreadyPending));
            Assert.That(duplicate,
                Is.TypeOf<OrganizationInvitationCreationResult.Rejection>());
            Assert.That(invitationCount, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ExpiredInvitationNoLongerDeduplicatesOrReservesASeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "expiry-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Expiry Organization",
            SignupTime.AddDays(1));
        await using (var initialContext = database.CreateContext())
        {
            await using var serviceFixture6 = CreateService(database, SignupTime.AddDays(2));
            var initial = await serviceFixture6.Service
                .CreateAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    "invitee@example.com",
                    OrganizationRole.Member);
            Assert.That(initial.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.Created));
        }

        await using var context = database.CreateContext();
        await using var serviceFixture7 = CreateService(database, SignupTime.AddDays(9));
        var replacement = await serviceFixture7.Service
            .CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "INVITEE@example.com",
                OrganizationRole.Admin);
        var invitationCount = await context.OrganizationInvitations.CountAsync();

        Assert.Multiple(() =>
        {
            Assert.That(replacement.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.Created));
            Assert.That(invitationCount, Is.EqualTo(2));
        });
    }

    [Test]
    public async Task CancelledInvitationNoLongerDeduplicatesOrReservesASeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancelled-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancelled Organization",
            SignupTime.AddDays(1));
        await using (var initialContext = database.CreateContext())
        {
            await using var serviceFixture8 = CreateService(database, SignupTime.AddDays(2));
            var initial = await serviceFixture8.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "cancelled@example.com",
                OrganizationRole.Member);
            var initialSuccess = RequireSuccess(initial);
            await initialContext.OrganizationInvitations
                .Where(invitation => invitation.Id == initialSuccess.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    invitation => invitation.CancelledAt,
                    SignupTime.AddDays(3)));
        }

        await using var context = database.CreateContext();
        await using var serviceFixture9 = CreateService(database, SignupTime.AddDays(3));
        var service = serviceFixture9.Service;
        for (var index = 0; index < 3; index++)
        {
            var preload = await service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                $"active-{index}@example.com",
                OrganizationRole.Member);
            RequireSuccess(preload);
        }

        var replacement = await service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "CANCELLED@example.com",
            OrganizationRole.Admin);
        RequireSuccess(replacement);

        await using var verificationContext = database.CreateContext();
        var invitations = await verificationContext.OrganizationInvitations.ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(invitations, Has.Length.EqualTo(5));
            Assert.That(invitations.Count(invitation => invitation.CancelledAt is null),
                Is.EqualTo(4));
            Assert.That(
                invitations.Count(invitation =>
                    invitation.NormalizedEmail == "CANCELLED@EXAMPLE.COM"),
                Is.EqualTo(2));
        });
    }

    [Test]
    public async Task OrganizationWithoutActiveTransferredTrialCannotReserveTrialSeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "capacity-owner", "owner@example.com");
        await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Trial Organization",
            SignupTime.AddDays(1));
        var unlicensedOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Unlicensed Organization",
            SignupTime.AddDays(2));
        await using var context = database.CreateContext();

        await using var serviceFixture10 = CreateService(database, SignupTime.AddDays(3));
        var result = await serviceFixture10.Service
            .CreateAsync(
                owner.UserId,
                unlicensedOrganization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member);

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationCreationStatus.NoActiveSeatCapacity));
    }

    [Test]
    public async Task TrialEndInstantDoesNotProvideInvitationCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "ended-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Ended Trial Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();

        await using var serviceFixture11 = CreateService(database, SignupTime.AddDays(30));
        var result = await serviceFixture11.Service
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
    public async Task TransferredTrialAllowsAtMostFiveAssignedAndPendingSeats()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "limit-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Limit Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture12 = CreateService(database, SignupTime.AddDays(2));
        var service = serviceFixture12.Service;
        var seatlessResults = new List<OrganizationInvitationCreationResult>();
        for (var index = 0; index < 2; index++)
        {
            seatlessResults.Add(
                await service.CreateAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    $"seatless-{index}@example.com",
                    OrganizationRole.Admin,
                    assignProductSeat: false));
        }
        var results = new List<OrganizationInvitationCreationResult>();
        for (var index = 0; index < 5; index++)
        {
            results.Add(
                await service.CreateAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    $"invitee-{index}@example.com",
                    OrganizationRole.Member));
        }
        var invitationCount = await context.OrganizationInvitations.CountAsync();
        var outboxCount = await context.OutboxMessages.CountAsync();

        Assert.Multiple(() =>
        {
            Assert.That(
                seatlessResults.Select(result => result.Status),
                Is.All.EqualTo(OrganizationInvitationCreationStatus.Created));
            Assert.That(
                results.Take(4).Select(result => result.Status),
                Is.All.EqualTo(OrganizationInvitationCreationStatus.Created));
            Assert.That(
                results[4].Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.SeatCapacityReached));
            Assert.That(invitationCount, Is.EqualTo(6));
            Assert.That(outboxCount, Is.EqualTo(6));
        });
    }

    [Test]
    public async Task ConcurrentInvitationsCannotOverbookLastTransferredTrialSeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "race-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "race-admin", "admin@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Race Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin);
        await using (var preloadContext = database.CreateContext())
        {
            await using var serviceFixture13 = CreateService(database, SignupTime.AddDays(2));
            var preloadService = serviceFixture13.Service;
            for (var index = 0; index < 2; index++)
            {
                var preload = await preloadService.CreateAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    $"preload-{index}@example.com",
                    OrganizationRole.Member);
                Assert.That(preload.Status,
                    Is.EqualTo(OrganizationInvitationCreationStatus.Created));
            }
        }

        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        await using var serviceFixture14 = CreateService(
                database,
                SignupTime.AddDays(3),
                new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations"));
        await using var serviceFixture15 = CreateService(
                database,
                SignupTime.AddDays(3),
                new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations"));
        var results = await Task.WhenAll(
            serviceFixture14.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "owner-invite@example.com",
                OrganizationRole.Member),
            serviceFixture15.Service.CreateAsync(
                admin.UserId,
                organization.OrganizationId,
                "admin-invite@example.com",
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
        await using var verificationContext = database.CreateContext();
        var outboxMessages = await verificationContext.OutboxMessages.ToArrayAsync();
        var invitationCount = await verificationContext.OrganizationInvitations.CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(invitationCount, Is.EqualTo(3));
            Assert.That(outboxMessages, Has.Length.EqualTo(3));
            Assert.That(
                results.OfType<OrganizationInvitationCreationResult.Success>()
                    .Select(result => result.CorrelationId),
                Is.SubsetOf(outboxMessages.Select(message => message.CorrelationId)));
        });
    }

    [Test]
    public async Task FailureAfterSavingRollsBackInvitationAndAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "rollback-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Rollback Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var serviceFixture16 = CreateService(database, SignupTime.AddDays(2), new ThrowAfterSaveInterceptor());
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await serviceFixture16.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member));

        await using var verificationContext = database.CreateContext();
        var invitationCount = await verificationContext.OrganizationInvitations.CountAsync();
        var auditCount = await verificationContext.AuditRecords.CountAsync();
        var deliveryCount = await verificationContext.OutboxMessages.CountAsync();
        var queuedMessageCount = await verificationContext.Set<RebusOutboxMessage>().CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(invitationCount, Is.Zero);
            Assert.That(auditCount, Is.EqualTo(baselineAuditCount));
            Assert.That(deliveryCount, Is.Zero);
            Assert.That(queuedMessageCount, Is.Zero);
        });
    }

    [Test]
    public async Task DatabaseRequiresUniqueInvitationSecretHashes()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "secret-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Secret Organization",
            SignupTime.AddDays(1));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var service = serviceTest.Service;
        var first = await service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "first@example.com",
            OrganizationRole.Member);

        var firstInvitation = RequireSuccess(first);
        await using var collisionContext = database.CreateContext();
        collisionContext.OrganizationInvitations.Add(OrganizationInvitation.Create(
            organization.OrganizationId,
            owner.UserId,
            "second@example.com",
            OrganizationRole.Member,
            firstInvitation.Secret.Hash,
            SignupTime.AddDays(2),
            assignProductSeat: false));
        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await collisionContext.SaveChangesAsync());

        Assert.That(exception!.InnerException, Is.TypeOf<PostgresException>());
        Assert.That(
            ((PostgresException)exception.InnerException!).ConstraintName,
            Is.EqualTo("ux_organization_invitations_secret_hash"));
        await using var verificationContext = database.CreateContext();
        Assert.That(await verificationContext.OrganizationInvitations.CountAsync(), Is.EqualTo(1));
    }

    [Test]
    public async Task DatabaseRejectsPartiallyAcceptedInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "state-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "State Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture17 = CreateService(database, SignupTime.AddDays(2));
        var created = await serviceFixture17.Service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            OrganizationRole.Member);
        var success = RequireSuccess(created);

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(invitation => invitation.Id == success.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    invitation => invitation.AcceptedAt,
                    SignupTime.AddDays(3))));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_organization_invitations_terminal_state"));
    }

    [Test]
    public async Task DatabaseRejectsInvitationWithNonPositiveLifetime()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "window-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Window Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture18 = CreateService(database, SignupTime.AddDays(2));
        var created = RequireSuccess(
            await serviceFixture18.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member));

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(invitation => invitation.Id == created.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    invitation => invitation.ExpiresAt,
                    SignupTime.AddDays(2))));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_organization_invitations_valid_window"));
    }

    [Test]
    public async Task DatabaseRejectsNonPositiveInvitationReservedSeatCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "reserved-capacity-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Reserved Capacity Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture19 = CreateService(database, SignupTime.AddDays(2));
        var created = RequireSuccess(
            await serviceFixture19.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member));

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(invitation => invitation.Id == created.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    invitation => invitation.ReservedSeatCapacity,
                    0)));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo(DatabaseConstraintNames.OrganizationInvitationReservedSeatCapacity));
    }

    [Test]
    public async Task DatabaseRejectsPositiveCapacityForSeatlessInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "seatless-capacity-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seatless Capacity Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture20 = CreateService(database, SignupTime.AddDays(2));
        var created = RequireSuccess(
            await serviceFixture20.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Admin,
                assignProductSeat: false));

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(invitation => invitation.Id == created.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    invitation => invitation.ReservedSeatCapacity,
                    1)));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo(DatabaseConstraintNames.OrganizationInvitationReservedSeatCapacity));
    }

    [Test]
    public async Task DatabaseRejectsLastSendBeforeInvitationCreation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "send-window-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Send Window Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture21 = CreateService(database, SignupTime.AddDays(2));
        var created = RequireSuccess(
            await serviceFixture21.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member));

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(invitation => invitation.Id == created.InvitationId)
                .ExecuteUpdateAsync(setters => setters
                    .SetProperty(
                        invitation => invitation.LastSentAt,
                        SignupTime.AddDays(2).AddSeconds(-1))
                    .SetProperty(
                        invitation => invitation.ExpiresAt,
                        SignupTime.AddDays(9).AddSeconds(-1))));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_organization_invitations_valid_window"));
    }

    [TestCase("2026-03-28T12:00:00+00:00")]
    [TestCase("2026-10-24T12:00:00+00:00")]
    public async Task DatabaseInvitationWindowRemainsAbsoluteAcrossCopenhagenDst(
        string createdAtText)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"dst-owner-{createdAtText}",
            $"dst-{Guid.CreateVersion7():N}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "DST Organization",
            SignupTime.AddDays(1));
        var createdAt = DateTimeOffset.Parse(
            createdAtText,
            CultureInfo.InvariantCulture,
            DateTimeStyles.AssumeUniversal);
        var invitation = OrganizationInvitation.Create(
            organization.OrganizationId,
            owner.UserId,
            "invitee@example.com",
            OrganizationRole.Member,
            new string('a', OrganizationInvitation.SecretHashLength),
            createdAt);
        invitation.ReserveSeatCapacity(5);
        await using var context = database.CreateContextWithCopenhagenTimeZone();
        context.OrganizationInvitations.Add(invitation);

        Assert.DoesNotThrowAsync(async () => await context.SaveChangesAsync());

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        Assert.That(
            persisted.ExpiresAt - persisted.LastSentAt,
            Is.EqualTo(OrganizationInvitation.Lifetime));
    }

    [Test]
    public async Task DatabaseRequiresExactlyOneHundredSixtyEightHourInvitationWindow()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "exact-window-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Exact Window Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(candidate => candidate.Id == invitation.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    candidate => candidate.ExpiresAt,
                    invitation.ExpiresAt.AddTicks(10))));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_organization_invitations_valid_window"));
    }

    [Test]
    public async Task DatabaseRejectsTerminalTransitionsBeforeLastSend()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "last-send-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Last Send Organization",
            SignupTime.AddDays(1));
        var accepted = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "accepted@example.com");
        var cancelled = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "cancelled@example.com");
        var lastSentAt = SignupTime.AddDays(4);
        var terminalAt = SignupTime.AddDays(3);
        await using (var preparationContext = database.CreateContext())
        {
            var invitationIds = new[] { accepted.InvitationId, cancelled.InvitationId };
            await preparationContext.OrganizationInvitations
                .Where(invitation => invitationIds.Contains(invitation.Id))
                .ExecuteUpdateAsync(setters => setters
                    .SetProperty(invitation => invitation.LastSentAt, lastSentAt)
                    .SetProperty(invitation => invitation.ExpiresAt, lastSentAt.AddDays(7)));
        }

        PostgresException acceptanceException;
        await using (var acceptanceContext = database.CreateContext())
        {
            acceptanceException = Assert.ThrowsAsync<PostgresException>(
                async () => await acceptanceContext.OrganizationInvitations
                    .Where(invitation => invitation.Id == accepted.InvitationId)
                    .ExecuteUpdateAsync(setters => setters
                        .SetProperty(invitation => invitation.AcceptedAt, terminalAt)
                        .SetProperty(invitation => invitation.AcceptedByUserId, owner.UserId)))!;
        }

        PostgresException cancellationException;
        await using (var cancellationContext = database.CreateContext())
        {
            cancellationException = Assert.ThrowsAsync<PostgresException>(
                async () => await cancellationContext.OrganizationInvitations
                    .Where(invitation => invitation.Id == cancelled.InvitationId)
                    .ExecuteUpdateAsync(setters => setters.SetProperty(
                        invitation => invitation.CancelledAt,
                        terminalAt)))!;
        }

        Assert.Multiple(() =>
        {
            Assert.That(
                acceptanceException.ConstraintName,
                Is.EqualTo("ck_organization_invitations_terminal_state"));
            Assert.That(
                cancellationException.ConstraintName,
                Is.EqualTo("ck_organization_invitations_terminal_state"));
        });
    }

    [Test]
    public async Task DatabaseRejectsInvitationThatIsBothAcceptedAndCancelled()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "terminal-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Terminal Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture22 = CreateService(database, SignupTime.AddDays(2));
        var created = RequireSuccess(
            await serviceFixture22.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member));

        var terminalTime = SignupTime.AddDays(3);
        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(invitation => invitation.Id == created.InvitationId)
                .ExecuteUpdateAsync(setters => setters
                    .SetProperty(invitation => invitation.AcceptedAt, terminalTime)
                    .SetProperty(invitation => invitation.AcceptedByUserId, owner.UserId)
                    .SetProperty(invitation => invitation.CancelledAt, terminalTime)));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_organization_invitations_terminal_state"));
    }

    [Test]
    public async Task DatabaseRejectsAcceptanceBeforeInvitationCreation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "acceptance-time-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Acceptance Time Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture23 = CreateService(database, SignupTime.AddDays(2));
        var created = RequireSuccess(
            await serviceFixture23.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member));

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(invitation => invitation.Id == created.InvitationId)
                .ExecuteUpdateAsync(setters => setters
                    .SetProperty(invitation => invitation.AcceptedAt, SignupTime.AddDays(1))
                    .SetProperty(invitation => invitation.AcceptedByUserId, owner.UserId)));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_organization_invitations_terminal_state"));
    }

    [Test]
    public async Task DatabaseRejectsCancellationBeforeInvitationCreation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancellation-time-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Time Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture24 = CreateService(database, SignupTime.AddDays(2));
        var created = RequireSuccess(
            await serviceFixture24.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member));

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(invitation => invitation.Id == created.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    invitation => invitation.CancelledAt,
                    SignupTime.AddDays(1))));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_organization_invitations_terminal_state"));
    }

    [Test]
    public async Task DatabaseRejectsCancellationAtInvitationExpiry()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancellation-expiry-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Expiry Organization",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();
        await using var serviceFixture25 = CreateService(database, SignupTime.AddDays(2));
        var created = RequireSuccess(
            await serviceFixture25.Service.CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member));

        var exception = Assert.ThrowsAsync<PostgresException>(
            async () => await context.OrganizationInvitations
                .Where(invitation => invitation.Id == created.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    invitation => invitation.CancelledAt,
                    created.ExpiresAt)));

        Assert.That(
            exception!.ConstraintName,
            Is.EqualTo("ck_organization_invitations_terminal_state"));
    }

    [Test]
    public async Task UnknownActorCannotCreateInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();

        await using var serviceFixture26 = CreateService(database, SignupTime);
        Assert.ThrowsAsync<UserNotFoundException>(
            async () => await serviceFixture26.Service.CreateAsync(
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                "invitee@example.com",
                OrganizationRole.Member));
        Assert.That(await context.OrganizationInvitations.CountAsync(), Is.Zero);
        Assert.That(await context.OutboxMessages.CountAsync(), Is.Zero);
        Assert.That(await context.AuditRecords.CountAsync(), Is.Zero);
    }

    private static ServiceTestBase<OrganizationInvitationCreationService> CreateService(
        PostgresTestDatabase database,
        DateTimeOffset observedAt,
        params IInterceptor[] interceptors)
    {
        var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, observedAt, interceptors: interceptors);
        return serviceTest;
    }

    private static OrganizationInvitationCreationResult.Success RequireSuccess(
        OrganizationInvitationCreationResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationInvitationCreationResult.Success>());
        return (OrganizationInvitationCreationResult.Success)result;
    }

}
