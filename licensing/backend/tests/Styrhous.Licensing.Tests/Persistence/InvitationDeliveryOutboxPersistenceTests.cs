using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Infrastructure.Messaging;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Npgsql;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class InvitationDeliveryOutboxPersistenceTests
{
    [Test]
    public async Task InvitationCreationAtomicallyEnqueuesProtectedDelivery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "outbox-create-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Outbox Creation Organization",
            SignupTime.AddDays(1));
        var observedAt = SignupTime.AddDays(2);
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, observedAt);
        var protector = serviceTest.Services.GetRequiredService<DataProtectionOrganizationInvitationDeliveryProtector>();
        var result = await serviceTest.Service
            .CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                " Invitee@Example.com ",
                OrganizationRole.Admin);
        var success = RequireSuccess(result);

        await using var verificationContext = database.CreateContext();
        var outbox = await verificationContext.OutboxMessages.AsNoTracking().SingleAsync();
        var delivery = protector.Unprotect(outbox.ProtectedPayload);
        Assert.Multiple(() =>
        {
            Assert.That(outbox.Id.Version, Is.EqualTo(7));
            Assert.That(success.BackgroundWork.WorkId, Is.EqualTo(outbox.Id));
            Assert.That(
                success.BackgroundWork.Kind,
                Is.EqualTo(BackgroundWorkKind.OrganizationInvitationDelivery));
            Assert.That(success.BackgroundWork.OccurredAt, Is.EqualTo(observedAt));
            Assert.That(outbox.CorrelationId, Is.EqualTo(success.CorrelationId));
            Assert.That(outbox.SubjectId, Is.EqualTo(success.InvitationId));
            Assert.That(
                outbox.MessageType,
                Is.EqualTo(OutboxMessageTypes.OrganizationInvitationDelivery));
            Assert.That(outbox.OccurredAt, Is.EqualTo(observedAt));
            Assert.That(outbox.NotAfter, Is.EqualTo(success.ExpiresAt));
            Assert.That(outbox.DiscardedAt, Is.Null);
            Assert.That(outbox.DiscardReason, Is.Null);
            Assert.That(outbox.ProtectedPayload, Does.Not.Contain(success.Secret.Reveal()));
            Assert.That(outbox.ProtectedPayload, Does.Not.Contain("Invitee@Example.com"));
            Assert.That(delivery.Kind, Is.EqualTo(OrganizationInvitationDeliveryKind.Created));
            Assert.That(delivery.InvitationId, Is.EqualTo(success.InvitationId));
            Assert.That(delivery.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(delivery.Email, Is.EqualTo("Invitee@Example.com"));
            Assert.That(delivery.Role, Is.EqualTo(OrganizationRole.Admin));
            Assert.That(delivery.Secret.Reveal(), Is.EqualTo(success.Secret.Reveal()));
            Assert.That(delivery.ExpiresAt, Is.EqualTo(success.ExpiresAt));
            Assert.That(delivery.ToString(), Does.Not.Contain(success.Secret.Reveal()));
        });
    }

    [Test]
    public async Task InvitationResendAtomicallyEnqueuesReplacementDelivery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "outbox-resend-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Outbox Resend Organization",
            SignupTime.AddDays(1));
        await using var creationTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var protector = creationTest.Services.GetRequiredService<DataProtectionOrganizationInvitationDeliveryProtector>();
        var created = RequireSuccess(
            await creationTest.Service
                .CreateAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    "invitee@example.com",
                    OrganizationRole.Member));
        var unrelated = RequireSuccess(
            await creationTest.Service
                .CreateAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    "unrelated@example.com",
                    OrganizationRole.Member));
        await using var resendTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var resent = RequireSuccess(
            await resendTest.Service
                .ResendAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    created.InvitationId));

        await using var verificationContext = database.CreateContext();
        var outboxMessages = await verificationContext.OutboxMessages.AsNoTracking().ToArrayAsync();
        var outbox = outboxMessages.Single(
            message => message.CorrelationId == resent.CorrelationId);
        var superseded = outboxMessages.Single(
            message => message.CorrelationId == created.CorrelationId);
        var unrelatedOutbox = outboxMessages.Single(
            message => message.CorrelationId == unrelated.CorrelationId);
        var delivery = protector.Unprotect(outbox.ProtectedPayload);
        Assert.Multiple(() =>
        {
            Assert.That(outboxMessages, Has.Length.EqualTo(3));
            Assert.That(outbox.Id.Version, Is.EqualTo(7));
            Assert.That(resent.BackgroundWork.WorkId, Is.EqualTo(outbox.Id));
            Assert.That(
                resent.BackgroundWork.Kind,
                Is.EqualTo(BackgroundWorkKind.OrganizationInvitationDelivery));
            Assert.That(
                resent.BackgroundWork.OccurredAt,
                Is.EqualTo(SignupTime.AddDays(3)));
            Assert.That(outbox.SubjectId, Is.EqualTo(created.InvitationId));
            Assert.That(outbox.OccurredAt, Is.EqualTo(SignupTime.AddDays(3)));
            Assert.That(outbox.NotAfter, Is.EqualTo(resent.ExpiresAt));
            Assert.That(outbox.DiscardedAt, Is.Null);
            Assert.That(outbox.DiscardReason, Is.Null);
            Assert.That(superseded.DiscardedAt, Is.EqualTo(SignupTime.AddDays(3)));
            Assert.That(
                superseded.DiscardReason,
                Is.EqualTo(OutboxDiscardReason.Superseded));
            Assert.That(unrelatedOutbox.DiscardedAt, Is.Null);
            Assert.That(unrelatedOutbox.DiscardReason, Is.Null);
            Assert.That(outbox.ProtectedPayload, Does.Not.Contain(resent.Secret.Reveal()));
            Assert.That(delivery.Kind, Is.EqualTo(OrganizationInvitationDeliveryKind.Resent));
            Assert.That(delivery.InvitationId, Is.EqualTo(created.InvitationId));
            Assert.That(delivery.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(delivery.Email, Is.EqualTo("invitee@example.com"));
            Assert.That(delivery.Role, Is.EqualTo(OrganizationRole.Member));
            Assert.That(delivery.Secret.Reveal(), Is.EqualTo(resent.Secret.Reveal()));
            Assert.That(delivery.ExpiresAt, Is.EqualTo(resent.ExpiresAt));
        });
    }

    [Test]
    public async Task RejectedInvitationCreationDoesNotEnqueueDelivery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "outbox-reject-owner", "owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "outbox-reject-outsider",
            "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Outbox Private Organization",
            SignupTime.AddDays(1));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var result = await serviceTest.Service
            .CreateAsync(
                outsider.UserId,
                organization.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member);
        await using var verificationContext = database.CreateContext();

        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(OrganizationInvitationCreationStatus.OrganizationNotFound));
            Assert.That(verificationContext.OutboxMessages, Is.Empty);
        });
    }

    [Test]
    public async Task RejectedInvitationResendDoesNotEnqueueReplacementDelivery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "outbox-resend-reject-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Outbox Resend Rejection Organization",
            SignupTime.AddDays(1));
        await using var creationTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(2));
        var created = RequireSuccess(
            await creationTest.Service
                .CreateAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    "invitee@example.com",
                    OrganizationRole.Member));

        await using var resendTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var rejected = await resendTest.Service
            .ResendAsync(
                owner.UserId,
                organization.OrganizationId,
                Guid.CreateVersion7());

        await using var verificationContext = database.CreateContext();
        var outboxMessages = await verificationContext.OutboxMessages.AsNoTracking().ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                rejected.Status,
                Is.EqualTo(OrganizationInvitationResendStatus.InvitationNotFound));
            Assert.That(outboxMessages, Has.Length.EqualTo(1));
            Assert.That(outboxMessages[0].CorrelationId, Is.EqualTo(created.CorrelationId));
            Assert.That(
                outboxMessages[0].ProtectedPayload,
                Does.Not.Contain("rejected-resend-secret"));
        });
    }

    [Test]
    public async Task InvitationCreationFailureRollsBackDeliveryWithBusinessChanges()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "outbox-rollback-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Outbox Rollback Organization",
            SignupTime.AddDays(1));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database,
            SignupTime.AddDays(2),
            interceptors: [new ThrowAfterSaveInterceptor()]);

        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await serviceTest.Service.CreateAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    "invitee@example.com",
                    OrganizationRole.Member));

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(verificationContext.OrganizationInvitations, Is.Empty);
            Assert.That(verificationContext.OutboxMessages, Is.Empty);
        });
    }

    [Test]
    public async Task InvitationResendFailureRollsBackDeliveryWithBusinessChanges()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "outbox-resend-rollback-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Outbox Resend Rollback Organization",
            SignupTime.AddDays(1));
        await using var creationTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, SignupTime.AddDays(2));
        OrganizationInvitationCreationResult.Success created;
        created = RequireSuccess(await creationTest.Service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            OrganizationRole.Member));

        await using var resendTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
            database,
            SignupTime.AddDays(3),
            interceptors: [new ThrowAfterSaveInterceptor()]);
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await resendTest.Service.ResendAsync(
                    owner.UserId,
                    organization.OrganizationId,
                    created.InvitationId));

        await using var verificationContext = database.CreateContext();
        var invitation = await verificationContext.OrganizationInvitations
            .AsNoTracking()
            .SingleAsync();
        var outboxMessages = await verificationContext.OutboxMessages
            .AsNoTracking()
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(invitation.SecretHash, Is.EqualTo(created.Secret.Hash));
            Assert.That(invitation.LastSentAt, Is.EqualTo(SignupTime.AddDays(2)));
            Assert.That(outboxMessages, Has.Length.EqualTo(1));
            Assert.That(outboxMessages[0].CorrelationId, Is.EqualTo(created.CorrelationId));
        });
    }

    [Test]
    public async Task InvitationCancellationDiscardsPendingDelivery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "outbox-cancel-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Outbox Cancellation Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        var unrelated = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "unrelated@example.com",
            SignupTime.AddDays(2));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCancellationService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var result = await serviceTest.Service
            .CancelAsync(owner.UserId, organization.OrganizationId, invitation.InvitationId);

        await using var verificationContext = database.CreateContext();
        var outboxMessages = await verificationContext.OutboxMessages.AsNoTracking().ToArrayAsync();
        var outbox = outboxMessages.Single(
            message => message.SubjectId == invitation.InvitationId);
        var unrelatedOutbox = outboxMessages.Single(
            message => message.SubjectId == unrelated.InvitationId);
        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(OrganizationInvitationCancellationStatus.Cancelled));
            Assert.That(outbox.DiscardedAt, Is.EqualTo(SignupTime.AddDays(3)));
            Assert.That(
                outbox.DiscardReason,
                Is.EqualTo(OutboxDiscardReason.InvitationCancelled));
            Assert.That(unrelatedOutbox.DiscardedAt, Is.Null);
            Assert.That(unrelatedOutbox.DiscardReason, Is.Null);
        });
    }

    [Test]
    public async Task InvitationAcceptanceDiscardsPendingDelivery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "outbox-accept-owner", "owner@example.com");
        var invitee = await SignUpAsync(
            database,
            "outbox-accept-invitee",
            "invitee@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Outbox Acceptance Organization",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        var unrelated = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "unrelated@example.com",
            SignupTime.AddDays(2));
        await using var serviceTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
            database, SignupTime.AddDays(3));
        var result = await serviceTest.Service
            .AcceptAsync(invitee.UserId, invitation.Secret.Reveal());

        await using var verificationContext = database.CreateContext();
        var outboxMessages = await verificationContext.OutboxMessages.AsNoTracking().ToArrayAsync();
        var outbox = outboxMessages.Single(
            message => message.SubjectId == invitation.InvitationId);
        var unrelatedOutbox = outboxMessages.Single(
            message => message.SubjectId == unrelated.InvitationId);
        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(OrganizationInvitationAcceptanceStatus.Accepted));
            Assert.That(outbox.DiscardedAt, Is.EqualTo(SignupTime.AddDays(3)));
            Assert.That(
                outbox.DiscardReason,
                Is.EqualTo(OutboxDiscardReason.InvitationAccepted));
            Assert.That(unrelatedOutbox.DiscardedAt, Is.Null);
            Assert.That(unrelatedOutbox.DiscardReason, Is.Null);
        });
    }

    [TestCase(InvalidLifecycleMutation.ExpiryAtOccurrence)]
    [TestCase(InvalidLifecycleMutation.DiscardReasonWithoutTimestamp)]
    [TestCase(InvalidLifecycleMutation.DiscardTimestampWithoutReason)]
    [TestCase(InvalidLifecycleMutation.DiscardBeforeOccurrence)]
    [TestCase(InvalidLifecycleMutation.DeliveredAndDiscarded)]
    public async Task DatabaseRejectsInvalidLifecycleMutation(
        InvalidLifecycleMutation mutation)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, $"outbox-lifecycle-{mutation}", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            $"Outbox Lifecycle {mutation}",
            SignupTime.AddDays(1));
        var invitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            SignupTime.AddDays(2));
        await using var context = database.CreateContext();
        var message = await context.OutboxMessages.SingleAsync(
            candidate => candidate.SubjectId == invitation.InvitationId);

        Func<Task> invalidUpdate = mutation switch
        {
            InvalidLifecycleMutation.ExpiryAtOccurrence => async () =>
                await context.OutboxMessages
                    .Where(candidate => candidate.SubjectId == invitation.InvitationId)
                    .ExecuteUpdateAsync(setters => setters.SetProperty(
                        candidate => candidate.NotAfter,
                        message.OccurredAt)),
            InvalidLifecycleMutation.DiscardReasonWithoutTimestamp => async () =>
                await context.OutboxMessages
                    .Where(candidate => candidate.SubjectId == invitation.InvitationId)
                    .ExecuteUpdateAsync(setters => setters.SetProperty(
                        candidate => candidate.DiscardReason,
                        OutboxDiscardReason.Superseded)),
            InvalidLifecycleMutation.DiscardTimestampWithoutReason => async () =>
                await context.OutboxMessages
                    .Where(candidate => candidate.SubjectId == invitation.InvitationId)
                    .ExecuteUpdateAsync(setters => setters.SetProperty(
                        candidate => candidate.DiscardedAt,
                        message.OccurredAt)),
            InvalidLifecycleMutation.DiscardBeforeOccurrence => async () =>
                await context.OutboxMessages
                    .Where(candidate => candidate.SubjectId == invitation.InvitationId)
                    .ExecuteUpdateAsync(setters => setters
                        .SetProperty(
                            candidate => candidate.DiscardedAt,
                            message.OccurredAt.AddSeconds(-1))
                        .SetProperty(
                            candidate => candidate.DiscardReason,
                            OutboxDiscardReason.Superseded)),
            InvalidLifecycleMutation.DeliveredAndDiscarded => async () =>
                await context.OutboxMessages
                    .Where(candidate => candidate.SubjectId == invitation.InvitationId)
                    .ExecuteUpdateAsync(setters => setters
                        .SetProperty(
                            candidate => candidate.DeliveredAt,
                            message.OccurredAt)
                        .SetProperty(
                            candidate => candidate.DiscardedAt,
                            message.OccurredAt)
                        .SetProperty(
                            candidate => candidate.DiscardReason,
                            OutboxDiscardReason.Superseded)),
            _ => throw new ArgumentOutOfRangeException(nameof(mutation)),
        };

        var exception = Assert.ThrowsAsync<PostgresException>(invalidUpdate);

        Assert.That(exception!.ConstraintName, Is.EqualTo("ck_outbox_messages_lifecycle"));
    }

    private static OrganizationInvitationCreationResult.Success RequireSuccess(
        OrganizationInvitationCreationResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationInvitationCreationResult.Success>());
        return (OrganizationInvitationCreationResult.Success)result;
    }

    private static OrganizationInvitationResendResult.Success RequireSuccess(
        OrganizationInvitationResendResult result)
    {
        Assert.That(result, Is.TypeOf<OrganizationInvitationResendResult.Success>());
        return (OrganizationInvitationResendResult.Success)result;
    }

    public enum InvalidLifecycleMutation
    {
        ExpiryAtOccurrence,
        DiscardReasonWithoutTimestamp,
        DiscardTimestampWithoutReason,
        DiscardBeforeOccurrence,
        DeliveredAndDiscarded,
    }

}
