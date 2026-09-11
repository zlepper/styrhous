using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationCancellationPersistenceTests
{

    [Test]
    public async Task OwnerCancelsPendingInvitationWithAuditInSameTransaction()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        var observedAt = SignupTime.AddDays(3).ToOffset(TimeSpan.FromHours(2));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCancellationService>.ForDatabase(
            database, observedAt);
        var service = serviceTest.Service;

        var result = await service.CancelAsync(
            owner.UserId,
            organization.OrganizationId,
            invitation.InvitationId);
        var success = RequireSuccess(result);

        Assert.Multiple(() =>
        {
            Assert.That(success.InvitationId, Is.EqualTo(invitation.InvitationId));
            Assert.That(success.CorrelationId.Version, Is.EqualTo(7));
            Assert.That(success.CancelledAt, Is.EqualTo(observedAt));
            Assert.That(success.CancelledAt.Offset, Is.EqualTo(TimeSpan.Zero));
        });
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        var audit = await verificationContext.AuditRecords.SingleAsync(
            record => record.CorrelationId == success.CorrelationId);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.CancelledAt, Is.EqualTo(observedAt.ToUniversalTime()));
            Assert.That(audit.Action, Is.EqualTo(AuditAction.OrganizationInvitationCancelled));
            Assert.That(audit.TargetType, Is.EqualTo(AuditTargetType.OrganizationInvitation));
            Assert.That(audit.TargetId, Is.EqualTo(invitation.InvitationId));
            Assert.That(audit.ActorUserId, Is.EqualTo(owner.UserId));
            Assert.That(audit.OccurredAt, Is.EqualTo(observedAt.ToUniversalTime()));
        });
    }

    [TestCase(OrganizationRole.Admin, OrganizationInvitationCancellationStatus.Cancelled)]
    [TestCase(
        OrganizationRole.Member,
        OrganizationInvitationCancellationStatus.InsufficientPermission)]
    [TestCase(
        (OrganizationRole)999,
        OrganizationInvitationCancellationStatus.InsufficientPermission)]
    public async Task OnlyOwnersAndAdminsCanCancelInvitations(
        OrganizationRole actorRole,
        OrganizationInvitationCancellationStatus expectedStatus)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"cancel-role-owner-{actorRole}",
            $"owner-{(int)actorRole}@example.com");
        var actor = await SignUpAsync(
            database,
            $"cancel-role-actor-{actorRole}",
            $"actor-{(int)actorRole}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Role Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(database, organization, actor.UserId, actorRole);
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        await using var context = database.CreateContext();

        await using var serviceFixture1 = CreateCancellationService(database, SignupTime.AddDays(3));
        var result = await serviceFixture1.Service.CancelAsync(
            actor.UserId,
            organization.OrganizationId,
            invitation.InvitationId);

        Assert.That(result.Status, Is.EqualTo(expectedStatus));
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        Assert.That(
            persisted.CancelledAt is not null,
            Is.EqualTo(expectedStatus == OrganizationInvitationCancellationStatus.Cancelled));
    }

    [Test]
    public async Task NonMemberCannotDiscoverOrganizationThroughCancellation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-private-owner", "owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "cancel-private-outsider",
            "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Private Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        await using var context = database.CreateContext();

        await using var serviceFixture2 = CreateCancellationService(database, SignupTime.AddDays(3));
        var result = await serviceFixture2.Service.CancelAsync(
            outsider.UserId,
            organization.OrganizationId,
            invitation.InvitationId);

        Assert.That(
            result.Status,
            Is.EqualTo(OrganizationInvitationCancellationStatus.OrganizationNotFound));
        Assert.That((await context.OrganizationInvitations.SingleAsync()).CancelledAt, Is.Null);
    }

    [Test]
    public async Task UnknownCrossOrganizationAndRepeatedInvitationsShareNotFoundStatus()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-not-found-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Primary Organization",
            SignupTime.AddDays(1));
        var otherOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Other Organization",
            SignupTime.AddDays(2));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        await using var context = database.CreateContext();
        await using var serviceFixture3 = CreateCancellationService(database, SignupTime.AddDays(3));
        var service = serviceFixture3.Service;

        var unknown = await service.CancelAsync(
            owner.UserId,
            organization.OrganizationId,
            Guid.CreateVersion7());
        var crossOrganization = await service.CancelAsync(
            owner.UserId,
            otherOrganization.OrganizationId,
            invitation.InvitationId);
        var cancelled = await service.CancelAsync(
            owner.UserId,
            organization.OrganizationId,
            invitation.InvitationId);
        var repeated = await service.CancelAsync(
            owner.UserId,
            organization.OrganizationId,
            invitation.InvitationId);

        Assert.Multiple(() =>
        {
            Assert.That(unknown.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.InvitationNotFound));
            Assert.That(crossOrganization.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.InvitationNotFound));
            Assert.That(cancelled.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.Cancelled));
            Assert.That(repeated.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.InvitationNotFound));
        });
    }

    [Test]
    public async Task ExpiredAndAcceptedInvitationsCannotBeCancelled()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-terminal-owner", "owner@example.com");
        var invitee = await SignUpAsync(database, "cancel-terminal-invitee", "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Terminal Organization",
            SignupTime.AddDays(1));
        var expired = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "expired@example.com",
            SignupTime.AddDays(2));
        var accepted = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(4));
        await using var context = database.CreateContext();
        await context.OrganizationInvitations
            .Where(invitation => invitation.Id == accepted.InvitationId)
            .ExecuteUpdateAsync(setters => setters
                .SetProperty(invitation => invitation.AcceptedAt, SignupTime.AddDays(5))
                .SetProperty(invitation => invitation.AcceptedByUserId, invitee.UserId));
        await using var serviceFixture4 = CreateCancellationService(database, expired.ExpiresAt);
        var service = serviceFixture4.Service;

        var expiredResult = await service.CancelAsync(
            owner.UserId,
            organization.OrganizationId,
            expired.InvitationId);
        var acceptedResult = await service.CancelAsync(
            owner.UserId,
            organization.OrganizationId,
            accepted.InvitationId);

        Assert.Multiple(() =>
        {
            Assert.That(expiredResult.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.InvitationNotFound));
            Assert.That(acceptedResult.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.InvitationNotFound));
        });
    }

    [Test]
    public async Task CancellationAfterTrialExpiryStillFreesReservedSeatAndEmail()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-capacity-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Capacity Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "replace@example.com",
            SignupTime.AddDays(29));
        await using var context = database.CreateContext();

        await using var serviceFixture5 = CreateCancellationService(database, SignupTime.AddDays(31));
        var cancellation = await serviceFixture5.Service
            .CancelAsync(
                owner.UserId,
                organization.OrganizationId,
                invitation.InvitationId);

        Assert.That(
            cancellation.Status,
            Is.EqualTo(OrganizationInvitationCancellationStatus.Cancelled));
        await using var verificationContext = database.CreateContext();
        Assert.That((await verificationContext.OrganizationInvitations.SingleAsync()).CancelledAt,
            Is.EqualTo(SignupTime.AddDays(31)));
    }

    [Test]
    public async Task CancelledInvitationFreesEmailAndLastTrialSeatImmediately()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-replace-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Replacement Organization",
            SignupTime.AddDays(1));
        var invitations = new List<OrganizationInvitationCreationResult.Success>();
        for (var index = 0; index < 4; index++)
        {
            invitations.Add(
                await CreateInvitationAsync(
                    database,
                    owner.UserId,
                    organization.OrganizationId,
                    index == 0 ? "replace@example.com" : $"pending-{index}@example.com",
                    SignupTime.AddDays(2)));
        }

        await using var context = database.CreateContext();
        await using var serviceFixture6 = CreateCancellationService(database, SignupTime.AddDays(3));
        var cancellation = await serviceFixture6.Service
            .CancelAsync(
                owner.UserId,
                organization.OrganizationId,
                invitations[0].InvitationId);
        await using var creationTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var replacement = await creationTest.Service
            .CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "REPLACE@example.com",
                OrganizationRole.Admin);

        Assert.Multiple(() =>
        {
            Assert.That(cancellation.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.Cancelled));
            Assert.That(replacement.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.Created));
        });
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.Count(invitation => invitation.CancelledAt is null),
                Is.EqualTo(4));
            Assert.That(
                persisted.Count(invitation =>
                    invitation.NormalizedEmail == "REPLACE@EXAMPLE.COM"),
                Is.EqualTo(2));
        });
    }

    [Test]
    public async Task ConcurrentCancellationChangesAndAuditsInvitationExactlyOnce()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-race-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "cancel-race-admin", "admin@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Race Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin);
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        await using var serviceFixture7 = CreateCancellationService(
                database,
                SignupTime.AddDays(3),
                new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations"));
        await using var serviceFixture8 = CreateCancellationService(
                database,
                SignupTime.AddDays(3),
                new DatabaseCommandBarrierInterceptor(barrier, "UPDATE organizations"));
        var results = await Task.WhenAll(
            serviceFixture7.Service.CancelAsync(
                owner.UserId,
                organization.OrganizationId,
                invitation.InvitationId),
            serviceFixture8.Service.CancelAsync(
                admin.UserId,
                organization.OrganizationId,
                invitation.InvitationId));

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                results.Count(result =>
                    result.Status == OrganizationInvitationCancellationStatus.Cancelled),
                Is.EqualTo(1));
            Assert.That(
                results.Count(result =>
                    result.Status == OrganizationInvitationCancellationStatus.InvitationNotFound),
                Is.EqualTo(1));
        });
        await using var verificationContext = database.CreateContext();
        Assert.That(
            await verificationContext.AuditRecords.CountAsync(
                record => record.Action == AuditAction.OrganizationInvitationCancelled),
            Is.EqualTo(1));
    }

    [Test]
    public async Task OlderCancellationIsSupersededAfterNewerResendCommits()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-order-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "cancel-order-admin", "admin@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Ordering Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(database, organization, admin.UserId, OrganizationRole.Admin);
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com");
        var cancellationGate = new DatabaseCommandGate();
        await using var resendContext = database.CreateContext();

        await using var serviceFixture9 = CreateCancellationService(
                database,
                SignupTime.AddDays(3),
                new DatabaseCommandGateInterceptor(cancellationGate, "UPDATE organizations"));
        var cancellationTask = serviceFixture9.Service
            .CancelAsync(
                owner.UserId,
                organization.OrganizationId,
                invitation.InvitationId);
        await cancellationGate.WaitUntilReachedAsync();
        await using var resendTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
            database, SignupTime.AddDays(4));
        var resendResult = await resendTest.Service
            .ResendAsync(
                admin.UserId,
                organization.OrganizationId,
                invitation.InvitationId);
        var resendSuccess = resendResult as OrganizationInvitationResendResult.Success;
        cancellationGate.Release();
        var cancellationResult = await cancellationTask;

        Assert.Multiple(() =>
        {
            Assert.That(resendSuccess, Is.Not.Null);
            Assert.That(resendResult.Status, Is.EqualTo(OrganizationInvitationResendStatus.Resent));
            Assert.That(
                cancellationResult.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.Superseded));
        });
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OrganizationInvitations.SingleAsync();
        var resendAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.OrganizationInvitationResent);
        var cancellationAuditCount = await verificationContext.AuditRecords.CountAsync(
            record => record.Action == AuditAction.OrganizationInvitationCancelled);
        Assert.Multiple(() =>
        {
            Assert.That(persisted.SecretHash, Is.EqualTo(resendSuccess!.Secret.Hash));
            Assert.That(persisted.LastSentAt, Is.EqualTo(SignupTime.AddDays(4)));
            Assert.That(persisted.ExpiresAt, Is.EqualTo(SignupTime.AddDays(11)));
            Assert.That(persisted.CancelledAt, Is.Null);
            Assert.That(resendAuditCount, Is.EqualTo(1));
            Assert.That(cancellationAuditCount, Is.Zero);
        });
    }

    [Test]
    public async Task TerminalInvitationsMaskSupersessionFromOlderCancellation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-stale-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Stale Terminal Organization",
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
        await using (var preparationContext = database.CreateContext())
        {
            await using var resendTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
                database, SignupTime.AddDays(4));
            var resendService = resendTest.Service;
            Assert.That(
                (await resendService.ResendAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    accepted.InvitationId)).Status,
                Is.EqualTo(OrganizationInvitationResendStatus.Resent));
            Assert.That(
                (await resendService.ResendAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    cancelled.InvitationId)).Status,
                Is.EqualTo(OrganizationInvitationResendStatus.Resent));
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
        await using var serviceFixture10 = CreateCancellationService(database, SignupTime.AddDays(3));
        var service = serviceFixture10.Service;

        var acceptedResult = await service.CancelAsync(
            owner.UserId,
            organization.OrganizationId,
            accepted.InvitationId);
        var cancelledResult = await service.CancelAsync(
            owner.UserId,
            organization.OrganizationId,
            cancelled.InvitationId);

        Assert.Multiple(() =>
        {
            Assert.That(
                acceptedResult.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.InvitationNotFound));
            Assert.That(
                cancelledResult.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.InvitationNotFound));
        });
    }

    [Test]
    public async Task FailureAfterSavingRollsBackCancellationAndAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "cancel-rollback-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Cancellation Rollback Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var serviceFixture11 = CreateCancellationService(database, SignupTime.AddDays(3), new ThrowAfterSaveInterceptor());
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await serviceFixture11.Service
                .CancelAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    invitation.InvitationId));

        await using var verificationContext = database.CreateContext();
        var persistedInvitation = await verificationContext.OrganizationInvitations.SingleAsync();
        var persistedOutboxMessage = await verificationContext.OutboxMessages.SingleAsync();
        var auditCount = await verificationContext.AuditRecords.CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persistedInvitation.CancelledAt, Is.Null);
            Assert.That(persistedOutboxMessage.DiscardedAt, Is.Null);
            Assert.That(persistedOutboxMessage.DiscardReason, Is.Null);
            Assert.That(auditCount, Is.EqualTo(baselineAuditCount));
        });
    }

    [Test]
    public async Task UnknownActorCannotCancelInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();

        await using var serviceFixture12 = CreateCancellationService(database, SignupTime);
        Assert.ThrowsAsync<UserNotFoundException>(
            async () => await serviceFixture12.Service.CancelAsync(
                Guid.CreateVersion7(),
                Guid.CreateVersion7(),
                Guid.CreateVersion7()));
        Assert.That(await context.OrganizationInvitations.CountAsync(), Is.Zero);
        Assert.That(await context.OutboxMessages.CountAsync(), Is.Zero);
        Assert.That(await context.AuditRecords.CountAsync(), Is.Zero);
    }

    private static ServiceTestBase<OrganizationInvitationCancellationService> CreateCancellationService(
        PostgresTestDatabase database,
        DateTimeOffset observedAt,
        params IInterceptor[] interceptors)
    {
        var serviceTest = ServiceTestBase<OrganizationInvitationCancellationService>.ForDatabase(
            database, observedAt, interceptors: interceptors);
        return serviceTest;
    }

    private static OrganizationInvitationCancellationResult.Success RequireSuccess(
        OrganizationInvitationCancellationResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationInvitationCancellationResult.Success>());
        return (OrganizationInvitationCancellationResult.Success)result;
    }
}
