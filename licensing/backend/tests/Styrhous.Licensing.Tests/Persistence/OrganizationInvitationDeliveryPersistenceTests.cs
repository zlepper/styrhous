using System.Text.Json;
using Microsoft.AspNetCore.DataProtection;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Npgsql;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Infrastructure.Messaging;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationDeliveryPersistenceTests
{

    private static readonly DateTimeOffset ObservedAt =
        LicensingPersistenceScenario.SignupTime.AddDays(3);

    [Test]
    public async Task ClaimDecryptsCurrentDeliveryAndCompletionIsIdempotent()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-success");
        await using var serviceFixture1 = CreateStore(database);
        var store = serviceFixture1.Service;
        var leaseId = Guid.CreateVersion7();

        var claim = await store.TryAcquireAsync(
            scenario.OutboxMessageId,
            leaseId,
            TimeSpan.FromMinutes(5),
            CancellationToken.None);
        var completed = await store.CompleteAsync(
            scenario.OutboxMessageId,
            leaseId,
            ObservedAt.AddMinutes(1),
            CancellationToken.None);
        var duplicate = await store.TryAcquireAsync(
            scenario.OutboxMessageId,
            Guid.CreateVersion7(),
            TimeSpan.FromMinutes(5),
            CancellationToken.None);
        await using var context = database.CreateContext();
        var persisted = await context.OutboxMessages.AsNoTracking().SingleAsync(
            message => message.Id == scenario.OutboxMessageId);

        Assert.Multiple(() =>
        {
            Assert.That(claim.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
            Assert.That(claim.Claim!.OutboxMessageId, Is.EqualTo(scenario.OutboxMessageId));
            Assert.That(claim.Claim.LeaseId, Is.EqualTo(leaseId));
            Assert.That(claim.Claim.Delivery.InvitationId, Is.EqualTo(scenario.InvitationId));
            Assert.That(claim.Claim.Delivery.OrganizationId, Is.EqualTo(scenario.OrganizationId));
            Assert.That(claim.Claim.Delivery.Email, Is.EqualTo("invitee@example.com"));
            Assert.That(claim.Claim.Delivery.Secret.Hash, Is.EqualTo(scenario.SecretHash));
            Assert.That(completed, Is.True);
            Assert.That(duplicate.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.AlreadyDelivered));
            Assert.That(persisted.DeliveredAt, Is.EqualTo(ObservedAt.AddMinutes(1)));
            Assert.That(persisted.ProcessingLeaseId, Is.Null);
            Assert.That(persisted.ProcessingLeaseExpiresAt, Is.Null);
            Assert.That(persisted.ProcessingAttemptCount, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ActiveLeaseIsBusyAndExpiredLeaseCanBeReclaimed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-reclaim");
        var timeProvider = new MutableTimeProvider(ObservedAt);
        await using var serviceFixture2 = CreateStore(database, timeProvider);
        var store = serviceFixture2.Service;
        var firstLeaseId = Guid.CreateVersion7();
        var secondLeaseId = Guid.CreateVersion7();
        Assert.That(
            (await store.TryAcquireAsync(
                scenario.OutboxMessageId,
                firstLeaseId,
                TimeSpan.FromMinutes(5),
                CancellationToken.None)).Status,
            Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));

        timeProvider.UtcNow = ObservedAt.AddMinutes(4);
        var busy = await store.TryAcquireAsync(
            scenario.OutboxMessageId,
            secondLeaseId,
            TimeSpan.FromMinutes(5),
            CancellationToken.None);
        timeProvider.UtcNow = ObservedAt.AddMinutes(5);
        var reclaimed = await store.TryAcquireAsync(
            scenario.OutboxMessageId,
            secondLeaseId,
            TimeSpan.FromMinutes(5),
            CancellationToken.None);
        var oldCompletion = await store.CompleteAsync(
            scenario.OutboxMessageId,
            firstLeaseId,
            ObservedAt.AddMinutes(6),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(busy.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Busy));
            Assert.That(reclaimed.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
            Assert.That(reclaimed.Claim!.LeaseId, Is.EqualTo(secondLeaseId));
            Assert.That(oldCompletion, Is.False);
        });
    }

    [TestCase(LeaseFinalization.Complete)]
    [TestCase(LeaseFinalization.Release)]
    public async Task ReclaimedLeaseWinsForcedRaceWithStaleFinalization(
        LeaseFinalization finalization)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(
            database,
            $"delivery-stale-{finalization}");
        var timeProvider = new MutableTimeProvider(ObservedAt);
        await using var serviceFixture3 = CreateStore(database, timeProvider);
        var store = serviceFixture3.Service;
        var firstLeaseId = Guid.CreateVersion7();
        Assert.That(
            (await store.TryAcquireAsync(
                scenario.OutboxMessageId,
                firstLeaseId,
                TimeSpan.FromMinutes(5),
                CancellationToken.None)).Status,
            Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
        var staleReadGate = new DatabaseCommandGate();
        await using var serviceFixture4 = CreateStore(
            database,
            new FixedTimeProvider(ObservedAt),
            new DatabaseCommandGateInterceptor(
                staleReadGate,
                "FROM outbox_messages",
                DatabaseCommandInterceptionPhase.AfterReaderExecution));
        var staleStore = serviceFixture4.Service;
        var staleFinalization = finalization switch
        {
            LeaseFinalization.Complete => staleStore.CompleteAsync(
                scenario.OutboxMessageId,
                firstLeaseId,
                ObservedAt.AddMinutes(6),
                CancellationToken.None),
            LeaseFinalization.Release => staleStore.ReleaseAsync(
                scenario.OutboxMessageId,
                firstLeaseId,
                CancellationToken.None),
            _ => throw new ArgumentOutOfRangeException(nameof(finalization)),
        };
        OrganizationInvitationDeliveryClaimResult reclaimed;
        try
        {
            await staleReadGate.WaitUntilReachedAsync();
            timeProvider.UtcNow = ObservedAt.AddMinutes(5);
            reclaimed = await store.TryAcquireAsync(
                scenario.OutboxMessageId,
                Guid.CreateVersion7(),
                TimeSpan.FromMinutes(5),
                CancellationToken.None);
        }
        finally
        {
            staleReadGate.Release();
        }

        var staleResult = await staleFinalization;
        await using var context = database.CreateContext();
        var persisted = await context.OutboxMessages.AsNoTracking().SingleAsync(
            message => message.Id == scenario.OutboxMessageId);

        Assert.Multiple(() =>
        {
            Assert.That(reclaimed.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
            Assert.That(staleResult, Is.False);
            Assert.That(persisted.ProcessingLeaseId, Is.EqualTo(reclaimed.Claim!.LeaseId));
            Assert.That(persisted.ProcessingAttemptCount, Is.EqualTo(2));
            Assert.That(persisted.DeliveredAt, Is.Null);
        });
    }

    [Test]
    public async Task ReleasedLeaseCanBeClaimedImmediately()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-release");
        await using var serviceFixture5 = CreateStore(database);
        var store = serviceFixture5.Service;
        var firstLeaseId = Guid.CreateVersion7();
        Assert.That(
            (await store.TryAcquireAsync(
                scenario.OutboxMessageId,
                firstLeaseId,
                TimeSpan.FromMinutes(5),
                CancellationToken.None)).Status,
            Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));

        var released = await store.ReleaseAsync(
            scenario.OutboxMessageId,
            firstLeaseId,
            CancellationToken.None);
        var second = await store.TryAcquireAsync(
            scenario.OutboxMessageId,
            Guid.CreateVersion7(),
            TimeSpan.FromMinutes(5),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(released, Is.True);
            Assert.That(second.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
        });
    }

    [Test]
    public async Task ForcedConcurrentClaimsHaveOneWinnerAfterReload()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-concurrent");
        var readGate = new DatabaseCommandGate();
        await using var serviceFixture6 = CreateStore(
            database,
            new FixedTimeProvider(ObservedAt),
            new DatabaseCommandGateInterceptor(
                readGate,
                "FROM outbox_messages",
                DatabaseCommandInterceptionPhase.AfterReaderExecution));
        var blockedStore = serviceFixture6.Service;
        var blockedClaim = ClaimAsync(blockedStore, scenario.OutboxMessageId);
        OrganizationInvitationDeliveryClaimResult winningClaim;
        try
        {
            await readGate.WaitUntilReachedAsync();
            await using var serviceFixture7 = CreateStore(database);
            winningClaim = await ClaimAsync(serviceFixture7.Service, scenario.OutboxMessageId);
        }
        finally
        {
            readGate.Release();
        }

        var losingClaim = await blockedClaim;
        await using var context = database.CreateContext();
        var persisted = await context.OutboxMessages.AsNoTracking().SingleAsync(
            message => message.Id == scenario.OutboxMessageId);

        Assert.Multiple(() =>
        {
            Assert.That(winningClaim.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
            Assert.That(losingClaim.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Busy));
            Assert.That(persisted.ProcessingLeaseId, Is.EqualTo(winningClaim.Claim!.LeaseId));
            Assert.That(persisted.ProcessingAttemptCount, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task CancellationDiscardsPublishedClaimedDeliveryAndInvalidatesCompletion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-cancel");
        var timeProvider = new MutableTimeProvider(ObservedAt);
        await using var serviceFixture8 = CreateStore(database, timeProvider);
        var store = serviceFixture8.Service;
        var leaseId = Guid.CreateVersion7();
        Assert.That(
            (await store.TryAcquireAsync(
                scenario.OutboxMessageId,
                leaseId,
                TimeSpan.FromMinutes(5),
                CancellationToken.None)).Status,
            Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
        await using var cancellationTest = ServiceTestBase<OrganizationInvitationCancellationService>.ForDatabase(
            database, ObservedAt.AddMinutes(1));
        var cancellationResult = await cancellationTest.Service.CancelAsync(
            scenario.OwnerUserId,
            scenario.OrganizationId,
            scenario.InvitationId);
        Assert.That(cancellationResult.Status, Is.EqualTo(OrganizationInvitationCancellationStatus.Cancelled));

        var completed = await store.CompleteAsync(
            scenario.OutboxMessageId,
            leaseId,
            ObservedAt.AddMinutes(2),
            CancellationToken.None);
        timeProvider.UtcNow = ObservedAt.AddMinutes(2);
        var retry = await store.TryAcquireAsync(
            scenario.OutboxMessageId,
            Guid.CreateVersion7(),
            TimeSpan.FromMinutes(5),
            CancellationToken.None);
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OutboxMessages.AsNoTracking().SingleAsync(
            message => message.Id == scenario.OutboxMessageId);

        Assert.Multiple(() =>
        {
            Assert.That(completed, Is.False);
            Assert.That(retry.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Discarded));
            Assert.That(persisted.DiscardReason, Is.EqualTo(OutboxDiscardReason.InvitationCancelled));
            Assert.That(persisted.ProcessingLeaseId, Is.Null);
        });
    }

    [Test]
    public async Task AcceptanceDiscardsPublishedClaimedDeliveryAndInvalidatesCompletion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-accept");
        var invitee = await LicensingPersistenceScenario.SignUpAsync(
            database,
            "delivery-accept-invitee",
            "invitee@example.com");
        await using var serviceFixture9 = CreateStore(database);
        var store = serviceFixture9.Service;
        var leaseId = Guid.CreateVersion7();
        Assert.That(
            (await store.TryAcquireAsync(
                scenario.OutboxMessageId,
                leaseId,
                TimeSpan.FromMinutes(5),
                CancellationToken.None)).Status,
            Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
        await using var acceptanceTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
            database, ObservedAt.AddMinutes(1));
        var acceptance = await acceptanceTest.Service.AcceptAsync(
            invitee.UserId, scenario.Secret.Reveal());

        var completed = await store.CompleteAsync(
            scenario.OutboxMessageId,
            leaseId,
            ObservedAt.AddMinutes(2),
            CancellationToken.None);
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OutboxMessages.AsNoTracking().SingleAsync(
            message => message.Id == scenario.OutboxMessageId);

        Assert.Multiple(() =>
        {
            Assert.That(acceptance.Status, Is.EqualTo(OrganizationInvitationAcceptanceStatus.Accepted));
            Assert.That(completed, Is.False);
            Assert.That(persisted.DiscardReason, Is.EqualTo(OutboxDiscardReason.InvitationAccepted));
            Assert.That(persisted.ProcessingLeaseId, Is.Null);
        });
    }

    [Test]
    public async Task ResendInvalidatesPublishedClaimedGenerationAndCreatesDeliverableReplacement()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-resend");
        var timeProvider = new MutableTimeProvider(ObservedAt);
        await using var serviceFixture10 = CreateStore(database, timeProvider);
        var store = serviceFixture10.Service;
        var oldLeaseId = Guid.CreateVersion7();
        Assert.That(
            (await store.TryAcquireAsync(
                scenario.OutboxMessageId,
                oldLeaseId,
                TimeSpan.FromMinutes(5),
                CancellationToken.None)).Status,
            Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
        var resend = await LicensingPersistenceScenario.ResendInvitationAsync(
            database,
            scenario.OwnerUserId,
            scenario.OrganizationId,
            scenario.InvitationId,
            ObservedAt.AddMinutes(1));
        await using var context = database.CreateContext();
        var replacementId = await context.OutboxMessages
            .Where(message => message.SubjectId == scenario.InvitationId
                && message.DiscardedAt == null)
            .Select(message => message.Id)
            .SingleAsync();

        timeProvider.UtcNow = ObservedAt.AddMinutes(2);
        var oldResult = await store.TryAcquireAsync(
            scenario.OutboxMessageId,
            Guid.CreateVersion7(),
            TimeSpan.FromMinutes(5),
            CancellationToken.None);
        var replacement = await store.TryAcquireAsync(
            replacementId,
            Guid.CreateVersion7(),
            TimeSpan.FromMinutes(5),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(oldResult.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Discarded));
            Assert.That(replacement.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
            Assert.That(replacement.Claim!.Delivery.Secret.Hash, Is.EqualTo(resend.Secret.Hash));
            Assert.That(replacement.Claim.Delivery.Secret.Hash, Is.Not.EqualTo(scenario.SecretHash));
        });
    }

    [Test]
    public async Task TerminalAndUnknownRowsReturnWithoutClaims()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-status");
        var expired = OutboxMessage.Enqueue(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            OutboxMessageTypes.OrganizationInvitationDelivery,
            "unused-protected-payload",
            ObservedAt.AddHours(-2),
            ObservedAt.AddHours(-1));
        var discarded = OutboxMessage.Enqueue(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            OutboxMessageTypes.OrganizationInvitationDelivery,
            "unused-protected-payload",
            ObservedAt.AddHours(-2));
        Assert.That(
            discarded.TryDiscard(
                OutboxDiscardReason.InvitationCancelled,
                ObservedAt.AddHours(-1)),
            Is.True);
        await using (var context = database.CreateContext())
        {
            context.OutboxMessages.AddRange(expired, discarded);
            await context.SaveChangesAsync();
        }

        await using var serviceFixture11 = CreateStore(database);
        var store = serviceFixture11.Service;
        var missing = await ClaimAsync(store, Guid.CreateVersion7());
        var expiredResult = await ClaimAsync(store, expired.Id);
        var discardedResult = await ClaimAsync(store, discarded.Id);

        Assert.Multiple(() =>
        {
            Assert.That(missing.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.NotFound));
            Assert.That(expiredResult.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Expired));
            Assert.That(discardedResult.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Discarded));
        });
    }

    [Test]
    public async Task SpecializedStoreCannotCompleteOrReleaseAnotherOutboxKind()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var foreign = OutboxMessage.Enqueue(
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "different-work-kind",
            "different-protected-payload",
            ObservedAt.AddMinutes(-1));
        var leaseId = Guid.CreateVersion7();
        Assert.That(
            foreign.TryAcquireProcessingLease(
                leaseId,
                ObservedAt,
                ObservedAt.AddMinutes(5)),
            Is.True);
        await using (var context = database.CreateContext())
        {
            context.OutboxMessages.Add(foreign);
            await context.SaveChangesAsync();
        }

        await using var serviceFixture12 = CreateStore(database);
        var store = serviceFixture12.Service;
        var claim = await ClaimAsync(store, foreign.Id);
        var completed = await store.CompleteAsync(
            foreign.Id,
            leaseId,
            ObservedAt.AddMinutes(1),
            CancellationToken.None);
        var released = await store.ReleaseAsync(
            foreign.Id,
            leaseId,
            CancellationToken.None);
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OutboxMessages.AsNoTracking().SingleAsync();

        Assert.Multiple(() =>
        {
            Assert.That(claim.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.NotFound));
            Assert.That(completed, Is.False);
            Assert.That(released, Is.False);
            Assert.That(persisted.ProcessingLeaseId, Is.EqualTo(leaseId));
            Assert.That(persisted.DeliveredAt, Is.Null);
        });
    }

    [Test]
    public async Task DecryptableEnvelopeMismatchIsQuarantined()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-envelope-mismatch");
        await using (var context = database.CreateContext())
        {
            var message = await context.OutboxMessages.SingleAsync(
                candidate => candidate.Id == scenario.OutboxMessageId);
            var mismatched = OrganizationInvitationDelivery.Restore(
                OrganizationInvitationDeliveryKind.Created,
                Guid.CreateVersion7(),
                scenario.OrganizationId,
                "invitee@example.com",
                OrganizationRole.Member,
                scenario.Secret.Reveal(),
                message.NotAfter!.Value);
            await using var serviceFixture13 = CreateProtector(database);
            await context.OutboxMessages
                .Where(candidate => candidate.Id == scenario.OutboxMessageId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    candidate => candidate.ProtectedPayload,
                    serviceFixture13.Service.Protect(mismatched)));
        }

        await using var serviceFixture14 = CreateStore(database);
        var result = await ClaimAsync(serviceFixture14.Service, scenario.OutboxMessageId);
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OutboxMessages.AsNoTracking().SingleAsync();
        var queuedBefore = await verificationContext.Set<RebusOutboxMessage>().CountAsync();
        await using var upgradeFixture = ServiceTestBase<NativeOutboxUpgrade>.ForDatabaseWithBackgroundQueue(database, ObservedAt, "test-queue");
        await upgradeFixture.Service.EnqueueLegacyWorkAsync(default);
        var queuedAfter = await verificationContext.Set<RebusOutboxMessage>().CountAsync();

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Undeliverable));
            Assert.That(
                persisted.DiscardReason,
                Is.EqualTo(OutboxDiscardReason.UndeliverableProtectedPayload));
            Assert.That(persisted.ProcessingAttemptCount, Is.Zero);
            Assert.That(queuedAfter, Is.EqualTo(queuedBefore));
        });
    }

    [Test]
    public async Task UndefinedRoleInProtectedPayloadIsQuarantined()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-undefined-role");
        await using (var context = database.CreateContext())
        {
            var message = await context.OutboxMessages.SingleAsync(
                candidate => candidate.Id == scenario.OutboxMessageId);
            var payload = JsonSerializer.Serialize(
                new
                {
                    SchemaVersion = 1,
                    Kind = OrganizationInvitationDeliveryKind.Created.ToString(),
                    scenario.InvitationId,
                    scenario.OrganizationId,
                    Email = "invitee@example.com",
                    Role = "999",
                    Secret = scenario.Secret.Reveal(),
                    ExpiresAt = message.NotAfter!.Value,
                });
            await using var protectorFixture = CreateProtector(database);
            var protectedPayload = protectorFixture.Services
                .GetRequiredService<IDataProtectionProvider>()
                .CreateProtector("Styrhous.Licensing.OrganizationInvitationDelivery")
                .Protect(payload);
            await context.OutboxMessages
                .Where(candidate => candidate.Id == scenario.OutboxMessageId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    candidate => candidate.ProtectedPayload,
                    protectedPayload));
        }

        await using var storeFixture = CreateStore(database);
        var result = await ClaimAsync(storeFixture.Service, scenario.OutboxMessageId);
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OutboxMessages.AsNoTracking().SingleAsync(
            message => message.Id == scenario.OutboxMessageId);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Undeliverable));
            Assert.That(persisted.ProcessingLeaseId, Is.Null);
            Assert.That(persisted.ProcessingAttemptCount, Is.Zero);
            Assert.That(persisted.DeliveredAt, Is.Null);
            Assert.That(
                persisted.DiscardReason,
                Is.EqualTo(OutboxDiscardReason.UndeliverableProtectedPayload));
        });
    }

    [Test]
    public async Task TamperedPayloadIsNeverClaimed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-tampered");
        await using (var context = database.CreateContext())
        {
            await context.OutboxMessages
                .Where(message => message.Id == scenario.OutboxMessageId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    message => message.ProtectedPayload,
                    "tampered-protected-payload"));
        }

        await using var serviceFixture15 = CreateStore(database);
        var result = await ClaimAsync(serviceFixture15.Service, scenario.OutboxMessageId);
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OutboxMessages.AsNoTracking().SingleAsync(
            message => message.Id == scenario.OutboxMessageId);
        var queuedBefore = await verificationContext.Set<RebusOutboxMessage>().CountAsync();
        await using var upgradeFixture = ServiceTestBase<NativeOutboxUpgrade>.ForDatabaseWithBackgroundQueue(database, ObservedAt, "test-queue");
        await upgradeFixture.Service.EnqueueLegacyWorkAsync(default);
        var queuedAfter = await verificationContext.Set<RebusOutboxMessage>().CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Undeliverable));
            Assert.That(persisted.ProcessingLeaseId, Is.Null);
            Assert.That(persisted.ProcessingAttemptCount, Is.Zero);
            Assert.That(persisted.DeliveredAt, Is.Null);
            Assert.That(
                persisted.DiscardReason,
                Is.EqualTo(OutboxDiscardReason.UndeliverableProtectedPayload));
            Assert.That(queuedAfter, Is.EqualTo(queuedBefore));
        });
    }

    [Test]
    public async Task TimeIsResampledAfterSerializationAndExpiredInvitationIsNotClaimed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-serialized-expiry");
        var serializationGate = new DatabaseCommandGate();
        var timeProvider = new MutableTimeProvider(ObservedAt);
        await using var serviceFixture16 = CreateStore(
            database,
            timeProvider,
            new DatabaseCommandGateInterceptor(serializationGate, "UPDATE organizations"));
        var store = serviceFixture16.Service;
        var claim = ClaimAsync(store, scenario.OutboxMessageId);
        try
        {
            await serializationGate.WaitUntilReachedAsync();
            timeProvider.UtcNow = scenario.ExpiresAt;
        }
        finally
        {
            serializationGate.Release();
        }

        var result = await claim;
        await using var context = database.CreateContext();
        var persisted = await context.OutboxMessages.AsNoTracking().SingleAsync(
            message => message.Id == scenario.OutboxMessageId);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Expired));
            Assert.That(persisted.ProcessingLeaseId, Is.Null);
            Assert.That(persisted.ProcessingAttemptCount, Is.Zero);
        });
    }

    [Test]
    public async Task ProcessingLeaseIsCappedAtInvitationDeliveryDeadline()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-capped-lease");
        await using var serviceFixture17 = CreateStore(database);
        var store = serviceFixture17.Service;

        var result = await store.TryAcquireAsync(
            scenario.OutboxMessageId,
            Guid.CreateVersion7(),
            TimeSpan.FromDays(30),
            CancellationToken.None);
        await using var context = database.CreateContext();
        var leaseExpiresAt = await context.OutboxMessages
            .Where(message => message.Id == scenario.OutboxMessageId)
            .Select(message => message.ProcessingLeaseExpiresAt)
            .SingleAsync();

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Acquired));
            Assert.That(leaseExpiresAt, Is.EqualTo(scenario.ExpiresAt));
        });
    }

    [Test]
    public async Task ProtectedDeliveryWithStaleSecretIsDiscarded()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-stale-secret");
        await ReplaceWithStaleSecretAsync(database, scenario);

        await using var serviceFixture18 = CreateStore(database);
        var result = await ClaimAsync(serviceFixture18.Service, scenario.OutboxMessageId);
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OutboxMessages.AsNoTracking().SingleAsync(
            message => message.Id == scenario.OutboxMessageId);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Discarded));
            Assert.That(persisted.DiscardReason, Is.EqualTo(OutboxDiscardReason.Superseded));
            Assert.That(persisted.ProcessingAttemptCount, Is.Zero);
        });
    }

    [Test]
    public async Task LegacyUpgradeWinningForcedRaceLeavesSupersededDiscardRetryable()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(database, "delivery-stale-publication-race");
        await ReplaceWithStaleSecretAsync(database, scenario);
        await using (var legacyContext = database.CreateContext())
        {
            await legacyContext.OutboxMessages.Where(message => message.Id == scenario.OutboxMessageId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(message => message.NativeOutboxEnqueued, false));
        }

        var invitationReadGate = new DatabaseCommandGate();
        await using var serviceFixture19 = CreateStore(
            database,
            new FixedTimeProvider(ObservedAt),
            new DatabaseCommandGateInterceptor(
                invitationReadGate,
                "FROM organization_invitations",
                DatabaseCommandInterceptionPhase.AfterReaderExecution));
        var blockedStore = serviceFixture19.Service;
        var blockedClaim = ClaimAsync(blockedStore, scenario.OutboxMessageId);

        try
        {
            await invitationReadGate.WaitUntilReachedAsync();
            await using var upgradeFixture = ServiceTestBase<NativeOutboxUpgrade>.ForDatabaseWithBackgroundQueue(database, ObservedAt, "test-queue");
            await upgradeFixture.Service.EnqueueLegacyWorkAsync(default);
        }
        finally
        {
            invitationReadGate.Release();
        }

        var racedResult = await blockedClaim;
        await using var serviceFixture20 = CreateStore(database);
        var retryResult = await ClaimAsync(serviceFixture20.Service, scenario.OutboxMessageId);
        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.OutboxMessages.AsNoTracking().SingleAsync(
            message => message.Id == scenario.OutboxMessageId);

        Assert.Multiple(() =>
        {
            Assert.That(persisted.NativeOutboxEnqueued, Is.True);
            Assert.That(racedResult.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Busy));
            Assert.That(retryResult.Status, Is.EqualTo(OrganizationInvitationDeliveryClaimStatus.Discarded));

            Assert.That(persisted.DiscardReason, Is.EqualTo(OutboxDiscardReason.Superseded));
            Assert.That(persisted.ProcessingAttemptCount, Is.Zero);
        });
    }

    [TestCase(InvalidOutboxState.PartialLease)]
    [TestCase(InvalidOutboxState.LeaseExpiresAtOccurrence)]
    [TestCase(InvalidOutboxState.NegativeProcessingAttempts)]
    [TestCase(InvalidOutboxState.DeliveredWithLease)]
    [TestCase(InvalidOutboxState.DeliveredBeforeOccurrence)]
    public async Task DatabaseRejectsInvalidProcessingState(InvalidOutboxState invalidState)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var scenario = await CreateScenarioAsync(
            database,
            $"delivery-constraint-{invalidState}");
        await using var context = database.CreateContext();
        var message = await context.OutboxMessages.AsNoTracking().SingleAsync(
            candidate => candidate.Id == scenario.OutboxMessageId);

        Func<Task> invalidUpdate = invalidState switch
        {
            InvalidOutboxState.PartialLease => async () =>
                await context.OutboxMessages
                    .Where(candidate => candidate.Id == scenario.OutboxMessageId)
                    .ExecuteUpdateAsync(setters => setters.SetProperty(
                        candidate => candidate.ProcessingLeaseId,
                        Guid.CreateVersion7())),
            InvalidOutboxState.LeaseExpiresAtOccurrence => async () =>
                await context.OutboxMessages
                    .Where(candidate => candidate.Id == scenario.OutboxMessageId)
                    .ExecuteUpdateAsync(setters => setters
                        .SetProperty(
                            candidate => candidate.ProcessingLeaseId,
                            Guid.CreateVersion7())
                        .SetProperty(
                            candidate => candidate.ProcessingLeaseExpiresAt,
                            message.OccurredAt)),
            InvalidOutboxState.NegativeProcessingAttempts => async () =>
                await context.OutboxMessages
                    .Where(candidate => candidate.Id == scenario.OutboxMessageId)
                    .ExecuteUpdateAsync(setters => setters.SetProperty(
                        candidate => candidate.ProcessingAttemptCount,
                        -1)),
            InvalidOutboxState.DeliveredWithLease => async () =>
                await context.OutboxMessages
                    .Where(candidate => candidate.Id == scenario.OutboxMessageId)
                    .ExecuteUpdateAsync(setters => setters
                        .SetProperty(
                            candidate => candidate.ProcessingLeaseId,
                            Guid.CreateVersion7())
                        .SetProperty(
                            candidate => candidate.ProcessingLeaseExpiresAt,
                            message.OccurredAt.AddMinutes(5))
                        .SetProperty(
                            candidate => candidate.DeliveredAt,
                            message.OccurredAt.AddMinutes(1))),
            InvalidOutboxState.DeliveredBeforeOccurrence => async () =>
                await context.OutboxMessages
                    .Where(candidate => candidate.Id == scenario.OutboxMessageId)
                    .ExecuteUpdateAsync(setters => setters.SetProperty(
                        candidate => candidate.DeliveredAt,
                        message.OccurredAt.AddSeconds(-1))),
            _ => throw new ArgumentOutOfRangeException(nameof(invalidState)),
        };

        var exception = Assert.ThrowsAsync<PostgresException>(invalidUpdate);
        var expectedConstraint = invalidState switch
        {
            InvalidOutboxState.PartialLease
                or InvalidOutboxState.LeaseExpiresAtOccurrence =>
                DatabaseConstraintNames.OutboxProcessingLease,
            InvalidOutboxState.NegativeProcessingAttempts =>
                DatabaseConstraintNames.OutboxProcessingAttempts,
            InvalidOutboxState.DeliveredWithLease =>
                DatabaseConstraintNames.OutboxProcessingState,
            InvalidOutboxState.DeliveredBeforeOccurrence =>
                DatabaseConstraintNames.OutboxLifecycle,
            _ => throw new ArgumentOutOfRangeException(nameof(invalidState)),
        };

        Assert.That(exception!.ConstraintName, Is.EqualTo(expectedConstraint));
    }

    public enum InvalidOutboxState
    {
        PartialLease,
        LeaseExpiresAtOccurrence,
        NegativeProcessingAttempts,
        DeliveredWithLease,
        DeliveredBeforeOccurrence,
    }

    public enum LeaseFinalization
    {
        Complete,
        Release,
    }

    private static Task<OrganizationInvitationDeliveryClaimResult> ClaimAsync(
        PostgresOrganizationInvitationDeliveryStore store,
        Guid outboxMessageId)
    {
        return store.TryAcquireAsync(
            outboxMessageId,
            Guid.CreateVersion7(),
            TimeSpan.FromMinutes(5),
            CancellationToken.None);
    }

    private static ServiceTestBase<PostgresOrganizationInvitationDeliveryStore> CreateStore(
        PostgresTestDatabase database,
        TimeProvider? timeProvider = null,
        params IInterceptor[] interceptors)
    {
        var storeTest = WorkerServiceTestBase.ForDatabase<PostgresOrganizationInvitationDeliveryStore>(
            database,
            ObservedAt,
            timeProvider is null
                ? null
                : services =>
                {
                    services.RemoveAll<TimeProvider>();
                    services.AddSingleton(timeProvider);
                },
            interceptors);
        return storeTest;
    }

    private static ServiceTestBase<DataProtectionOrganizationInvitationDeliveryProtector> CreateProtector(
        PostgresTestDatabase database)
    {
        var protectorTest = WorkerServiceTestBase.ForDatabase<DataProtectionOrganizationInvitationDeliveryProtector>(
            database, ObservedAt);
        return protectorTest;
    }

    private static async Task<DeliveryScenario> CreateScenarioAsync(
        PostgresTestDatabase database,
        string subject)
    {
        var owner = await LicensingPersistenceScenario.SignUpAsync(
            database,
            $"{subject}-owner",
            $"{subject}-owner@example.com");
        var organization = await LicensingPersistenceScenario.CreateOrganizationAsync(
            database,
            owner.UserId,
            $"{subject} organization",
            LicensingPersistenceScenario.SignupTime.AddDays(1));
        var invitation = await LicensingPersistenceScenario.CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "invitee@example.com",
            LicensingPersistenceScenario.SignupTime.AddDays(2));
        await using var context = database.CreateContext();
        var outbox = await context.OutboxMessages
            .Where(message => message.SubjectId == invitation.InvitationId)
            .Select(message => new
            {
                message.Id,
                message.OccurredAt,
            })
            .SingleAsync();
        var invitationState = await context.OrganizationInvitations
            .Where(candidate => candidate.Id == invitation.InvitationId)
            .Select(candidate => new
            {
                candidate.SecretHash,
                candidate.ExpiresAt,
            })
            .SingleAsync();
        return new DeliveryScenario(
            owner.UserId,
            organization.OrganizationId,
            invitation.InvitationId,
            outbox.Id,
            invitation.Secret,
            invitationState.SecretHash,
            invitationState.ExpiresAt,
            outbox.OccurredAt);
    }

    private static async Task ReplaceWithStaleSecretAsync(
        PostgresTestDatabase database,
        DeliveryScenario scenario)
    {
        await using var context = database.CreateContext();
        var message = await context.OutboxMessages.SingleAsync(
            candidate => candidate.Id == scenario.OutboxMessageId);
        var stale = OrganizationInvitationDelivery.Restore(
            OrganizationInvitationDeliveryKind.Created,
            scenario.InvitationId,
            scenario.OrganizationId,
            "invitee@example.com",
            OrganizationRole.Member,
            "stale-secret",
            message.NotAfter!.Value);
        await using var serviceFixture21 = CreateProtector(database);
        await context.OutboxMessages
            .Where(candidate => candidate.Id == scenario.OutboxMessageId)
            .ExecuteUpdateAsync(setters => setters.SetProperty(
                candidate => candidate.ProtectedPayload,
                serviceFixture21.Service.Protect(stale)));
    }

    private sealed record DeliveryScenario(
        Guid OwnerUserId,
        Guid OrganizationId,
        Guid InvitationId,
        Guid OutboxMessageId,
        OrganizationInvitationSecret Secret,
        string SecretHash,
        DateTimeOffset ExpiresAt,
        DateTimeOffset OutboxOccurredAt);
}
