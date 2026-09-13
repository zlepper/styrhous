using Styrhous.Licensing.Infrastructure.Organizations;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationResendPersistenceTests
{

    [Test]
    public async Task OwnerRotatesInvitationSecretAndAuditsResendInSameTransaction()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        var observedAt = SignupTime.AddDays(3).ToOffset(TimeSpan.FromHours(2));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
            database, observedAt);
        var service = serviceTest.Service;

        var result = await service.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            invitation.InvitationId);
        var success = RequireSuccess(result);

        Assert.Multiple(() =>
        {
            Assert.That(success.InvitationId, Is.EqualTo(invitation.InvitationId));
            Assert.That(success.CorrelationId.Version, Is.EqualTo(7));
            Assert.That(success.Secret.Reveal(), Is.Not.Empty);
            Assert.That(success.ToString(), Does.Not.Contain(success.Secret.Reveal()));
            Assert.That(success.ExpiresAt, Is.EqualTo(observedAt.ToUniversalTime().AddDays(7)));
        });
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        var audit = await verificationContext.AuditRecords.SingleAsync(
            record => record.CorrelationId == success.CorrelationId);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.Id, Is.EqualTo(invitation.InvitationId));
            Assert.That(persisted.SecretHash, Is.EqualTo(success.Secret.Hash));
            Assert.That(persisted.SecretHash, Is.Not.EqualTo(success.Secret.Reveal()));
            Assert.That(persisted.CreatedAt, Is.EqualTo(SignupTime.AddDays(2)));
            Assert.That(persisted.LastSentAt, Is.EqualTo(observedAt.ToUniversalTime()));
            Assert.That(persisted.ExpiresAt, Is.EqualTo(observedAt.ToUniversalTime().AddDays(7)));
            Assert.That(audit.Action, Is.EqualTo(AuditAction.OrganizationInvitationResent));
            Assert.That(audit.TargetType, Is.EqualTo(AuditTargetType.OrganizationInvitation));
            Assert.That(audit.TargetId, Is.EqualTo(invitation.InvitationId));
            Assert.That(audit.ActorUserId, Is.EqualTo(owner.UserId));
            Assert.That(audit.OccurredAt, Is.EqualTo(observedAt.ToUniversalTime()));
        });
    }

    [TestCase(OrganizationRole.Admin, OrganizationInvitationResendStatus.Resent)]
    [TestCase(OrganizationRole.Member, OrganizationInvitationResendStatus.InsufficientPermission)]
    [TestCase((OrganizationRole)999, OrganizationInvitationResendStatus.InsufficientPermission)]
    public async Task OnlyOwnersAndAdminsCanResendInvitations(
        OrganizationRole actorRole,
        OrganizationInvitationResendStatus expectedStatus)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"resend-role-owner-{(int)actorRole}",
            $"owner-{(int)actorRole}@example.com");
        var actor = await SignUpAsync(
            database,
            $"resend-role-actor-{(int)actorRole}",
            $"actor-{(int)actorRole}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Role Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(database, organization, actor.UserId, actorRole);
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using var context = database.CreateContext();

        await using var serviceFixture1 = CreateResendService(database, SignupTime.AddDays(3));
        var result = await serviceFixture1.Service.ResendAsync(
            actor.UserId,
            organization.OrganizationId,
            invitation.InvitationId);

        Assert.That(result.Status, Is.EqualTo(expectedStatus));
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        Assert.That(
            persisted.LastSentAt == SignupTime.AddDays(3),
            Is.EqualTo(expectedStatus == OrganizationInvitationResendStatus.Resent));
    }

    [Test]
    public async Task NonMemberCannotDiscoverOrganizationThroughInvitationResend()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-private-owner", "owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "resend-private-outsider",
            "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Private Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using var context = database.CreateContext();

        await using var serviceFixture2 = CreateResendService(database, SignupTime.AddDays(3));
        var result = await serviceFixture2.Service.ResendAsync(
            outsider.UserId,
            organization.OrganizationId,
            invitation.InvitationId);

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationResendStatus.OrganizationNotFound));
        Assert.That((await context.OrganizationInvitations.SingleAsync()).LastSentAt,
            Is.EqualTo(SignupTime.AddDays(2)));
    }

    [Test]
    public async Task UnknownCrossOrganizationCancelledAndAcceptedInvitationsShareNotFoundStatus()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-terminal-owner", "owner@example.com");
        var acceptedUser = await SignUpAsync(
            database,
            "resend-terminal-user",
            "accepted@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Terminal Organization",
            SignupTime.AddDays(1));
        var otherOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Other Organization",
            SignupTime.AddDays(2));
        var cancelled = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "cancelled@example.com");
        var accepted = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "accepted@example.com");
        await using var context = database.CreateContext();
        await context.OrganizationInvitations
            .Where(invitation => invitation.Id == cancelled.InvitationId)
            .ExecuteUpdateAsync(setters => setters.SetProperty(
                invitation => invitation.CancelledAt,
                SignupTime.AddDays(3)));
        await context.OrganizationInvitations
            .Where(invitation => invitation.Id == accepted.InvitationId)
            .ExecuteUpdateAsync(setters => setters
                .SetProperty(invitation => invitation.AcceptedAt, SignupTime.AddDays(3))
                .SetProperty(invitation => invitation.AcceptedByUserId, acceptedUser.UserId));
        await using var serviceFixture3 = CreateResendService(database, SignupTime.AddDays(4));
        var service = serviceFixture3.Service;

        var results = new[]
        {
            await service.ResendAsync(
                owner.UserId,
                organization.OrganizationId,
                Guid.CreateVersion7()),
            await service.ResendAsync(
                owner.UserId,
                otherOrganization.OrganizationId,
                accepted.InvitationId),
            await service.ResendAsync(
                owner.UserId,
                organization.OrganizationId,
                cancelled.InvitationId),
            await service.ResendAsync(
                owner.UserId,
                organization.OrganizationId,
                accepted.InvitationId),
        };

        Assert.That(
            results.Select(result => result.Status),
            Is.All.EqualTo(OrganizationInvitationResendStatus.InvitationNotFound));
    }

    [Test]
    public async Task TerminalInvitationsMaskSupersessionFromOlderResend()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-stale-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Stale Terminal Organization",
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
        OrganizationInvitationResendResult.Success acceptedResend;
        OrganizationInvitationResendResult.Success cancelledResend;
        await using (var preparationTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
                         database, SignupTime.AddDays(4)))
        {
            var service = preparationTest.Service;
            acceptedResend = RequireSuccess(
                await service.ResendAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    accepted.InvitationId));
            cancelledResend = RequireSuccess(
                await service.ResendAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    cancelled.InvitationId));
            await using var verificationContext = database.CreateContext();
            await verificationContext.OrganizationInvitations
                .Where(invitation => invitation.Id == accepted.InvitationId)
                .ExecuteUpdateAsync(setters => setters
                    .SetProperty(invitation => invitation.AcceptedAt, SignupTime.AddDays(5))
                    .SetProperty(invitation => invitation.AcceptedByUserId, owner.UserId));
            await verificationContext.OrganizationInvitations
                .Where(invitation => invitation.Id == cancelled.InvitationId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    invitation => invitation.CancelledAt,
                    SignupTime.AddDays(5)));
        }

        await using var context = database.CreateContext();
        await using var serviceFixture4 = CreateResendService(database, SignupTime.AddDays(3));
        var staleService = serviceFixture4.Service;
        var acceptedResult = await staleService.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            accepted.InvitationId);
        var cancelledResult = await staleService.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            cancelled.InvitationId);

        Assert.Multiple(() =>
        {
            Assert.That(
                acceptedResult.Status,
                Is.EqualTo(OrganizationInvitationResendStatus.InvitationNotFound));
            Assert.That(
                cancelledResult.Status,
                Is.EqualTo(OrganizationInvitationResendStatus.InvitationNotFound));
        });
        await using var followupVerificationContext = database.CreateContext();
        var persisted = await followupVerificationContext.OrganizationInvitations
            .OrderBy(invitation => invitation.Id)
            .ToArrayAsync();
        var expectedHashes = new[] { acceptedResend.Secret.Hash, cancelledResend.Secret.Hash };
        var resendAuditCount = await followupVerificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.OrganizationInvitationResent);
        Assert.Multiple(() =>
        {
            Assert.That(
                persisted.Select(invitation => invitation.SecretHash),
                Is.EquivalentTo(expectedHashes));
            Assert.That(
                persisted.Select(invitation => invitation.LastSentAt),
                Is.All.EqualTo(SignupTime.AddDays(4)));
            Assert.That(
                persisted.Select(invitation => invitation.ExpiresAt),
                Is.All.EqualTo(SignupTime.AddDays(11)));
            Assert.That(resendAuditCount, Is.EqualTo(2));
        });
    }

    [Test]
    public async Task ResendRejectsAnInviteeWhoBecameAMember()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-member-owner", "owner@example.com");
        var invitee = await SignUpAsync(database, "resend-member-invitee", "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Member Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "INVITEE@example.com");
        await AddOrganizationMemberAsync(
            database,
            organization,
            invitee.UserId,
            OrganizationRole.Member);
        await using var context = database.CreateContext();

        await using var serviceFixture5 = CreateResendService(database, SignupTime.AddDays(3));
        var result = await serviceFixture5.Service.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            invitation.InvitationId);

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationResendStatus.AlreadyMember));
    }

    [Test]
    public async Task ExpiredInvitationCannotReplaceAnotherPendingInvitationForTheSameEmail()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-email-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Email Organization",
            SignupTime.AddDays(1));
        var expired = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "INVITEE@example.com",
            SignupTime.AddDays(10));
        await using var context = database.CreateContext();

        await using var serviceFixture6 = CreateResendService(database, SignupTime.AddDays(10));
        var result = await serviceFixture6.Service.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            expired.InvitationId);

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationResendStatus.InvitationAlreadyPending));
    }

    [Test]
    public async Task InvitationExpiringAtTheObservedInstantDoesNotBlockResendByEmail()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-boundary-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Boundary Organization",
            SignupTime.AddDays(1));
        var expired = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        var target = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "INVITEE@example.com",
            expired.ExpiresAt);
        await using var context = database.CreateContext();

        await using var serviceFixture7 = CreateResendService(database, expired.ExpiresAt);
        var result = await serviceFixture7.Service.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            target.InvitationId);

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationResendStatus.Resent));
    }

    [Test]
    public async Task ActiveInvitationCanBeResentWhileItOwnsTheLastSeatReservation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-active-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Active Capacity Organization",
            SignupTime.AddDays(1));
        var cancelled = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "cancelled@example.com");
        await using (var cancellationContext = database.CreateContext())
        {
            await using var cancellationTest = ServiceTestBase<OrganizationInvitationCancellationService>.ForDatabase(
                database, SignupTime.AddDays(3));
            var cancellation = await cancellationTest.Service
                .CancelAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    cancelled.InvitationId);
            Assert.That(
                cancellation.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.Cancelled));
        }

        var invitations = new List<OrganizationInvitationCreationResult.Success>();
        for (var index = 0; index < 4; index++)
        {
            invitations.Add(
                await CreateInvitationAsync(
                    database,
                    owner.UserId,
                    organization.OrganizationId,
                    $"invitee-{index}@example.com",
                    SignupTime.AddDays(3)));
        }

        await using var context = database.CreateContext();
        await using var serviceFixture8 = CreateResendService(database, SignupTime.AddDays(4));
        var result = await serviceFixture8.Service.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            invitations[0].InvitationId);

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationResendStatus.Resent));
        Assert.That(await context.OrganizationInvitations.CountAsync(), Is.EqualTo(5));
    }

    [Test]
    public async Task ExpiredInvitationMustReacquireTrialCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-expired-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Expired Capacity Organization",
            SignupTime.AddDays(1));
        var expired = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "expired@example.com",
            SignupTime.AddDays(2));
        for (var index = 0; index < 4; index++)
        {
            await CreateInvitationAsync(
                database,
                owner.UserId,
                organization.OrganizationId,
                $"active-{index}@example.com",
                SignupTime.AddDays(10));
        }

        await using var context = database.CreateContext();
        await using var serviceFixture9 = CreateResendService(database, SignupTime.AddDays(10));
        var result = await serviceFixture9.Service.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            expired.InvitationId);

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationResendStatus.SeatCapacityReached));
        await using var verificationContext = database.CreateContext();
        Assert.That(
            (await verificationContext.OrganizationInvitations.SingleAsync(
                candidate => candidate.Id == expired.InvitationId)).LastSentAt,
            Is.EqualTo(SignupTime.AddDays(2)));
    }

    [Test]
    public async Task TrialEndInstantCannotReactivateExpiredInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-trial-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Trial Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using var context = database.CreateContext();

        await using var serviceFixture10 = CreateResendService(database, SignupTime.AddDays(30));
        var result = await serviceFixture10.Service.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            invitation.InvitationId);

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationResendStatus.NoActiveSeatCapacity));
    }

    [Test]
    public async Task SeatlessInvitationCanBeResentAfterTrialExpiryWithoutCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "resend-seatless-owner",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seatless Resend Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2),
            assignProductSeat: false);
        var observedAt = SignupTime.AddDays(30);
        await using var context = database.CreateContext();

        await using var serviceFixture11 = CreateResendService(database, observedAt);
        var result = await serviceFixture11.Service.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            invitation.InvitationId);

        Assert.That(result.Status, Is.EqualTo(OrganizationInvitationResendStatus.Resent));
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync(
            candidate => candidate.Id == invitation.InvitationId);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.AssignProductSeat, Is.False);
            Assert.That(persisted.ReservedSeatCapacity, Is.Zero);
            Assert.That(persisted.LastSentAt, Is.EqualTo(observedAt));
            Assert.That(persisted.ExpiresAt, Is.EqualTo(observedAt.Add(
                OrganizationInvitation.Lifetime)));
        });
    }

    [Test]
    public async Task ConcurrentExpiredResendsCannotOverbookLastTransferredTrialSeat()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-race-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "resend-race-admin", "admin@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Race Organization",
            SignupTime.AddDays(1));
        var firstExpired = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "expired-one@example.com",
            SignupTime.AddDays(2));
        var secondExpired = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "expired-two@example.com",
            SignupTime.AddDays(2));
        await AddOrganizationMemberAsync(database, organization, admin.UserId, OrganizationRole.Admin);
        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "active-one@example.com",
            SignupTime.AddDays(10));
        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "active-two@example.com",
            SignupTime.AddDays(10));
        int baselineOutboxCount;
        await using (var baselineContext = database.CreateContext())
        {
            baselineOutboxCount = await baselineContext.OutboxMessages.CountAsync();
        }
        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        await using var serviceFixture12 = CreateResendService(
                database,
                SignupTime.AddDays(10),
                new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations"));
        await using var serviceFixture13 = CreateResendService(
                database,
                SignupTime.AddDays(10),
                new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations"));
        var results = await Task.WhenAll(
            serviceFixture12.Service.ResendAsync(
                owner.UserId,
                organization.OrganizationId,
                firstExpired.InvitationId),
            serviceFixture13.Service.ResendAsync(
                admin.UserId,
                organization.OrganizationId,
                secondExpired.InvitationId));
        var success = results.OfType<OrganizationInvitationResendResult.Success>().Single();
        var losingInvitationId = success.InvitationId == firstExpired.InvitationId
            ? secondExpired.InvitationId
            : firstExpired.InvitationId;
        await using var verificationContext = database.CreateContext();
        var outboxMessages = await verificationContext.OutboxMessages.ToArrayAsync();
        var successfulSubjectMessages = outboxMessages
            .Where(message => message.SubjectId == success.InvitationId)
            .ToArray();
        var losingSubjectMessage = outboxMessages.Single(
            message => message.SubjectId == losingInvitationId);

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(results.Count(result => result.Status == OrganizationInvitationResendStatus.Resent),
                Is.EqualTo(1));
            Assert.That(results.Count(result => result.Status == OrganizationInvitationResendStatus.SeatCapacityReached),
                Is.EqualTo(1));
            Assert.That(outboxMessages, Has.Length.EqualTo(baselineOutboxCount + 1));
            Assert.That(
                outboxMessages.Count(message => message.CorrelationId == success.CorrelationId),
                Is.EqualTo(1));
            Assert.That(successfulSubjectMessages, Has.Length.EqualTo(2));
            Assert.That(
                successfulSubjectMessages.Count(
                    message => message.DiscardReason == OutboxDiscardReason.Superseded),
                Is.EqualTo(1));
            Assert.That(
                successfulSubjectMessages.Count(message => message.DiscardedAt is null),
                Is.EqualTo(1));
            Assert.That(losingSubjectMessage.DiscardedAt, Is.Null);
            Assert.That(losingSubjectMessage.DiscardReason, Is.Null);
        });
    }

    [Test]
    public async Task OlderConcurrentResendIsSupersededAfterNewerResendCommits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-order-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "resend-order-admin", "admin@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Ordering Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(database, organization, admin.UserId, OrganizationRole.Admin);
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var olderGate = new DatabaseCommandGate();
        await using var newerContext = database.CreateContext();

        await using var serviceFixture14 = CreateResendService(
                database,
                SignupTime.AddDays(3),
                new DatabaseCommandGateInterceptor(olderGate, "UPDATE organizations"));
        var olderTask = serviceFixture14.Service.ResendAsync(
            owner.UserId,
            organization.OrganizationId,
            invitation.InvitationId);
        await olderGate.WaitUntilReachedAsync();
        await using var serviceFixture15 = CreateResendService(database, SignupTime.AddDays(4));
        var newerResult = await serviceFixture15.Service.ResendAsync(
            admin.UserId,
            organization.OrganizationId,
            invitation.InvitationId);
        olderGate.Release();
        var olderResult = await olderTask;
        var newerSuccess = RequireSuccess(newerResult);

        Assert.Multiple(() =>
        {
            Assert.That(newerResult.Status, Is.EqualTo(OrganizationInvitationResendStatus.Resent));
            Assert.That(
                olderResult.Status,
                Is.EqualTo(OrganizationInvitationResendStatus.Superseded));
        });
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        var resendAudits = await verificationContext.AuditRecords
            .Where(record => record.Action == AuditAction.OrganizationInvitationResent)
            .ToArrayAsync();
        var outboxMessages = await verificationContext.OutboxMessages.ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.SecretHash, Is.EqualTo(newerSuccess.Secret.Hash));
            Assert.That(persisted.LastSentAt, Is.EqualTo(SignupTime.AddDays(4)));
            Assert.That(persisted.ExpiresAt, Is.EqualTo(SignupTime.AddDays(11)));
            Assert.That(resendAudits, Has.Length.EqualTo(1));
            Assert.That(resendAudits[0].CorrelationId, Is.EqualTo(newerSuccess.CorrelationId));
            Assert.That(outboxMessages, Has.Length.EqualTo(2));
            Assert.That(
                outboxMessages.Count(
                    message => message.CorrelationId == newerSuccess.CorrelationId),
                Is.EqualTo(1));
            Assert.That(
                outboxMessages.Single(
                    message => message.CorrelationId != newerSuccess.CorrelationId)
                    .DiscardReason,
                Is.EqualTo(OutboxDiscardReason.Superseded));
        });
    }

    [Test]
    public async Task FailureAfterSavingRollsBackSecretWindowAndAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "resend-rollback-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Resend Rollback Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var serviceFixture16 = CreateResendService(database, SignupTime.AddDays(3), new ThrowAfterSaveInterceptor());
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await serviceFixture16.Service.ResendAsync(
                owner.UserId,
                organization.OrganizationId,
                invitation.InvitationId));

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        var auditCount = await verificationContext.AuditRecords.CountAsync();
        var deliveryCount = await verificationContext.OutboxMessages.CountAsync();
        var queuedMessageCount = await verificationContext.Set<RebusOutboxMessage>().CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.SecretHash, Is.EqualTo(invitation.Secret.Hash));
            Assert.That(persisted.LastSentAt, Is.EqualTo(SignupTime.AddDays(2)));
            Assert.That(auditCount, Is.EqualTo(baselineAuditCount));
            Assert.That(deliveryCount, Is.EqualTo(1));
            Assert.That(queuedMessageCount, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task UnknownActorCannotResendInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();

        await using var serviceFixture17 = CreateResendService(database, SignupTime);
        Assert.ThrowsAsync<UserNotFoundException>(
            async () => await serviceFixture17.Service.ResendAsync(
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                Guid.CreateVersion7()));
        Assert.That(await context.OrganizationInvitations.CountAsync(), Is.Zero);
        Assert.That(await context.OutboxMessages.CountAsync(), Is.Zero);
        Assert.That(await context.AuditRecords.CountAsync(), Is.Zero);
    }

    private static ServiceTestBase<OrganizationInvitationResendService> CreateResendService(
        PostgresTestDatabase database,
        DateTimeOffset observedAt,
        params IInterceptor[] interceptors)
    {
        var serviceTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
            database, observedAt, interceptors: interceptors);
        return serviceTest;
    }

    private static OrganizationInvitationResendResult.Success RequireSuccess(
        OrganizationInvitationResendResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationInvitationResendResult.Success>());
        return (OrganizationInvitationResendResult.Success)result;
    }

}
