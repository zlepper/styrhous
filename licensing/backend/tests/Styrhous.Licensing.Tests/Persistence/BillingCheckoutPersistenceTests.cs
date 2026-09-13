using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingCheckoutPersistenceTests
{
    private static readonly DateTimeOffset CheckoutTime = SignupTime.AddDays(3);

    [Test]
    public async Task PersonalOwnerCanPrepareOneSeatCheckoutWithoutAnApplicationMaximum()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-personal",
            "checkout-personal@example.com");

        await using (var context = database.CreateContext())
        {
            var store = new PostgresBillingCheckoutStore(context.CreateContextFactory());
            var result = await store.PrepareAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                seatQuantity: 1,
                retryOperationId: null,
                CheckoutTime,
                CancellationToken.None);

            Assert.That(result.Status, Is.EqualTo(BillingCheckoutPreparationStatus.Prepared));
            Assert.That(result.RequiredSeatQuantity, Is.EqualTo(1));
            Assert.That(result.Operation, Is.Not.Null);
            Assert.That(result.Operation!.Id.Version, Is.EqualTo(7));
            Assert.That(result.Operation.SeatQuantity, Is.EqualTo(1));
        }

        await using var verification = database.CreateContext();
        var operation = await verification.BillingOperations.SingleAsync();
        var audit = await verification.AuditRecords.SingleAsync(
            record => record.Action == AuditAction.BillingCheckoutStarted);
        Assert.Multiple(() =>
        {
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Pending));
            Assert.That(operation.ActorUserId, Is.EqualTo(signup.UserId));
            Assert.That(operation.CreatedAt, Is.EqualTo(CheckoutTime));
            Assert.That(
                operation.ExpiresAt,
                Is.EqualTo(CheckoutTime.Add(BillingOperation.CheckoutLifetime)));
            Assert.That(audit.TargetType, Is.EqualTo(AuditTargetType.BillingOperation));
            Assert.That(audit.TargetId, Is.EqualTo(operation.Id));
            Assert.That(audit.CorrelationId, Is.EqualTo(operation.Id));
        });
    }

    [Test]
    public async Task PersonalCheckoutRejectsEveryQuantityOtherThanOne()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-personal-quantity",
            "checkout-personal-quantity@example.com");
        await using var context = database.CreateContext();
        var store = new PostgresBillingCheckoutStore(context.CreateContextFactory());

        var result = await store.PrepareAsync(
            signup.UserId,
            signup.PersonalBillingAccountId,
            BillingCadence.Annual,
            seatQuantity: 2,
            retryOperationId: null,
            CheckoutTime,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                result.Status,
                Is.EqualTo(BillingCheckoutPreparationStatus.PersonalSeatQuantityInvalid));
            Assert.That(result.RequiredSeatQuantity, Is.EqualTo(1));
            Assert.That(context.BillingOperations, Is.Empty);
        });
    }

    [Test]
    public async Task OrganizationQuantityCoversAssignedSeatsAndOnlyActiveInvitations()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "checkout-org-owner",
            "checkout-org-owner@example.com");
        var member = await SignUpAsync(
            database,
            "checkout-org-member",
            "checkout-org-member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Checkout organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(1));
        var active = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "active-checkout@example.com",
            CheckoutTime.AddDays(-1));
        var expired = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "expired-checkout@example.com",
            CheckoutTime.AddHours(-2));
        var cancelled = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "cancelled-checkout@example.com",
            CheckoutTime.AddHours(-1));
        await using (var setup = database.CreateContext())
        {
            var expiredInvitation = await setup.OrganizationInvitations.SingleAsync(
                invitation => invitation.Id == expired.InvitationId);
            setup.Entry(expiredInvitation).Property(invitation => invitation.CreatedAt)
                .CurrentValue = CheckoutTime.AddDays(-8);
            setup.Entry(expiredInvitation).Property(invitation => invitation.LastSentAt)
                .CurrentValue = CheckoutTime.AddDays(-8);
            setup.Entry(expiredInvitation).Property(invitation => invitation.ExpiresAt)
                .CurrentValue = CheckoutTime.AddDays(-1);
            var cancelledInvitation = await setup.OrganizationInvitations.SingleAsync(
                invitation => invitation.Id == cancelled.InvitationId);
            Assert.That(cancelledInvitation.TryCancel(CheckoutTime), Is.True);
            await setup.SaveChangesAsync();
        }

        await using var context = database.CreateContext();
        var store = new PostgresBillingCheckoutStore(context.CreateContextFactory());
        var tooSmall = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            BillingCadence.Monthly,
            seatQuantity: 2,
            retryOperationId: null,
            CheckoutTime,
            CancellationToken.None);
        var prepared = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            BillingCadence.Monthly,
            seatQuantity: 3,
            retryOperationId: null,
            CheckoutTime,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                tooSmall.Status,
                Is.EqualTo(BillingCheckoutPreparationStatus.SeatQuantityTooSmall));
            Assert.That(tooSmall.RequiredSeatQuantity, Is.EqualTo(3));
            Assert.That(prepared.Status, Is.EqualTo(BillingCheckoutPreparationStatus.Prepared));
            Assert.That(prepared.RequiredSeatQuantity, Is.EqualTo(3));
            Assert.That(prepared.Operation!.SeatQuantity, Is.EqualTo(3));
            Assert.That(active.InvitationId, Is.Not.EqualTo(expired.InvitationId));
        });
    }

    [Test]
    public async Task OrganizationCheckoutHasNoApplicationLevelMaximum()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "checkout-unbounded-owner",
            "checkout-unbounded-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Unbounded checkout",
            SignupTime.AddDays(1));
        await using var context = database.CreateContext();

        var result = await new PostgresBillingCheckoutStore(context.CreateContextFactory()).PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            BillingCadence.Annual,
            int.MaxValue,
            retryOperationId: null,
            CheckoutTime,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(BillingCheckoutPreparationStatus.Prepared));
            Assert.That(result.Operation!.SeatQuantity, Is.EqualTo(int.MaxValue));
        });
    }

    [TestCase(OrganizationRole.Admin)]
    [TestCase(OrganizationRole.Member)]
    public async Task OnlyOrganizationOwnerCanPrepareCheckout(OrganizationRole role)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"checkout-owner-{role}",
            $"checkout-owner-{role}@example.com");
        var actor = await SignUpAsync(
            database,
            $"checkout-actor-{role}",
            $"checkout-actor-{role}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            $"Checkout {role}",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(database, organization, actor.UserId, role);
        await using var context = database.CreateContext();

        var result = await new PostgresBillingCheckoutStore(context.CreateContextFactory()).PrepareAsync(
            actor.UserId,
            organization.BillingAccountId,
            BillingCadence.Monthly,
            seatQuantity: 2,
            retryOperationId: null,
            CheckoutTime,
            CancellationToken.None);

        Assert.That(
            result.Status,
            Is.EqualTo(BillingCheckoutPreparationStatus.InsufficientPermission));
        Assert.That(context.BillingOperations, Is.Empty);
    }

    [Test]
    public async Task UnknownOrInaccessibleAccountDoesNotDiscloseItsExistence()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "checkout-private-owner",
            "checkout-private-owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "checkout-private-outsider",
            "checkout-private-outsider@example.com");
        await using var context = database.CreateContext();
        var store = new PostgresBillingCheckoutStore(context.CreateContextFactory());

        var privateAccount = await store.PrepareAsync(
            outsider.UserId,
            owner.PersonalBillingAccountId,
            BillingCadence.Monthly,
            seatQuantity: 1,
            retryOperationId: null,
            CheckoutTime,
            CancellationToken.None);
        var missingAccount = await store.PrepareAsync(
            outsider.UserId,
            Guid.CreateVersion7(),
            BillingCadence.Monthly,
            seatQuantity: 1,
            retryOperationId: null,
            CheckoutTime,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                privateAccount.Status,
                Is.EqualTo(BillingCheckoutPreparationStatus.BillingAccountNotFound));
            Assert.That(
                missingAccount.Status,
                Is.EqualTo(BillingCheckoutPreparationStatus.BillingAccountNotFound));
        });
    }

    [TestCase(CommercialSubscriptionStatus.Active, false)]
    [TestCase(CommercialSubscriptionStatus.PastDue, false)]
    [TestCase(CommercialSubscriptionStatus.Unpaid, false)]
    [TestCase(CommercialSubscriptionStatus.Paused, false)]
    [TestCase(CommercialSubscriptionStatus.Incomplete, false)]
    [TestCase(CommercialSubscriptionStatus.Trialing, false)]
    [TestCase(CommercialSubscriptionStatus.Canceled, true)]
    [TestCase(CommercialSubscriptionStatus.IncompleteExpired, true)]
    public async Task OnlyTerminalSubscriptionsPermitAnotherCheckout(CommercialSubscriptionStatus status, bool allowed)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-existing",
            "checkout-existing@example.com");
        await using (var context = database.CreateContext())
        {
            context.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    signup.PersonalBillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_checkout_existing",
                        "sub_checkout_existing",
                        "price_checkout_existing",
                        status,
                        seatQuantity: 1,
                        cancelAtPeriodEnd: false,
                        SignupTime,
                        SignupTime.AddDays(1),
                        SignupTime.AddDays(2))));
            await context.SaveChangesAsync();
        }

        await using var checkoutContext = database.CreateContext();
        var result = await new PostgresBillingCheckoutStore(checkoutContext.CreateContextFactory()).PrepareAsync(
            signup.UserId,
            signup.PersonalBillingAccountId,
            BillingCadence.Monthly,
            seatQuantity: 1,
            retryOperationId: null,
            CheckoutTime,
            CancellationToken.None);

        Assert.That(
            result.Status,
            Is.EqualTo(allowed ? BillingCheckoutPreparationStatus.Prepared : BillingCheckoutPreparationStatus.SubscriptionAlreadyExists));
        if (allowed)
        {
            Assert.That(result.Operation!.PreviousSubscription!.ExternalCustomerId, Is.EqualTo("cus_checkout_existing"));

            Assert.That(checkoutContext.BillingOperations.Single().PreviousSubscription!.Status, Is.EqualTo(status));
        }
    }

    [Test]
    public async Task MatchingRetryAndRepeatedFreshRequestReuseTheLiveOperation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-retry",
            "checkout-retry@example.com");
        Guid operationId;
        await using (var firstContext = database.CreateContext())
        {
            var first = await new PostgresBillingCheckoutStore(firstContext.CreateContextFactory()).PrepareAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                seatQuantity: 1,
                retryOperationId: null,
                CheckoutTime,
                CancellationToken.None);
            operationId = first.Operation!.Id;
        }

        await using var retryContext = database.CreateContext();
        var store = new PostgresBillingCheckoutStore(retryContext.CreateContextFactory());
        var retry = await store.PrepareAsync(
            signup.UserId,
            signup.PersonalBillingAccountId,
            BillingCadence.Monthly,
            seatQuantity: 1,
            operationId,
            CheckoutTime.AddMinutes(1),
            CancellationToken.None);
        var repeatedFreshRequest = await store.PrepareAsync(
            signup.UserId,
            signup.PersonalBillingAccountId,
            BillingCadence.Monthly,
            seatQuantity: 1,
            retryOperationId: null,
            CheckoutTime.AddMinutes(1),
            CancellationToken.None);
        var mismatched = await store.PrepareAsync(
            signup.UserId,
            signup.PersonalBillingAccountId,
            BillingCadence.Annual,
            seatQuantity: 1,
            operationId,
            CheckoutTime.AddMinutes(1),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(retry.Status, Is.EqualTo(BillingCheckoutPreparationStatus.Prepared));
            Assert.That(retry.Operation!.Id, Is.EqualTo(operationId));
            Assert.That(
                repeatedFreshRequest.Status,
                Is.EqualTo(BillingCheckoutPreparationStatus.Prepared));
            Assert.That(repeatedFreshRequest.Operation!.Id, Is.EqualTo(operationId));
            Assert.That(
                mismatched.Status,
                Is.EqualTo(BillingCheckoutPreparationStatus.BillingOperationNotFound));
            Assert.That(retryContext.BillingOperations.Count(), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ChangedFreshRequestCannotCreateASecondLiveCheckout()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "checkout-live-owner",
            "checkout-live-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Live Checkout",
            SignupTime.AddDays(1));
        Guid operationId;
        await using (var firstContext = database.CreateContext())
        {
            var first = await new PostgresBillingCheckoutStore(firstContext.CreateContextFactory()).PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                BillingCadence.Monthly,
                1,
                retryOperationId: null,
                CheckoutTime,
                CancellationToken.None);
            operationId = first.Operation!.Id;
        }

        await using var changedContext = database.CreateContext();
        var changed = await new PostgresBillingCheckoutStore(changedContext.CreateContextFactory()).PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            BillingCadence.Annual,
            12,
            retryOperationId: null,
            CheckoutTime.AddMinutes(1),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(
                changed.Status,
                Is.EqualTo(BillingCheckoutPreparationStatus.CheckoutOperationInProgress));
            Assert.That(changed.Operation!.Id, Is.EqualTo(operationId));
            Assert.That(changed.Operation.Cadence, Is.EqualTo(BillingCadence.Monthly));
            Assert.That(changed.Operation.SeatQuantity, Is.EqualTo(1));
            Assert.That(changedContext.BillingOperations.Count(), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ConcurrentFreshRequestsSerializeToOneLiveOperation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-live-race",
            "checkout-live-race@example.com");
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        await using var firstContext = database.CreateContext(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE billing_accounts"));
        await using var secondContext = database.CreateContext(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE billing_accounts"));

        var results = await Task.WhenAll(
            new PostgresBillingCheckoutStore(firstContext.CreateContextFactory()).PrepareAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                retryOperationId: null,
                CheckoutTime,
                CancellationToken.None),
            new PostgresBillingCheckoutStore(secondContext.CreateContextFactory()).PrepareAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                retryOperationId: null,
                CheckoutTime,
                CancellationToken.None));

        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                results.Select(result => result.Status),
                Has.All.EqualTo(BillingCheckoutPreparationStatus.Prepared));
            Assert.That(
                results.Select(result => result.Operation!.Id).Distinct().Count(),
                Is.EqualTo(1));
            Assert.That(verification.BillingOperations.Count(), Is.EqualTo(1));
            Assert.That(
                verification.AuditRecords.Count(
                    record => record.Action == AuditAction.BillingCheckoutStarted),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task RetryRevalidatesSubscriptionAndCurrentOrganizationCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "checkout-revalidate-owner",
            "checkout-revalidate-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Revalidate Checkout",
            SignupTime.AddDays(1));
        Guid operationId;
        await using (var firstContext = database.CreateContext())
        {
            var first = await new PostgresBillingCheckoutStore(firstContext.CreateContextFactory()).PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                BillingCadence.Monthly,
                1,
                retryOperationId: null,
                CheckoutTime,
                CancellationToken.None);
            operationId = first.Operation!.Id;
        }

        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "checkout-new-reservation@example.com",
            CheckoutTime.AddMinutes(1));
        await using (var capacityContext = database.CreateContext())
        {
            var tooSmall = await new PostgresBillingCheckoutStore(capacityContext.CreateContextFactory()).PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                BillingCadence.Monthly,
                1,
                operationId,
                CheckoutTime.AddMinutes(2),
                CancellationToken.None);
            Assert.Multiple(() =>
            {
                Assert.That(
                    tooSmall.Status,
                    Is.EqualTo(
                        BillingCheckoutPreparationStatus.CheckoutOperationCapacityChanged));
                Assert.That(tooSmall.RequiredSeatQuantity, Is.EqualTo(2));
                Assert.That(tooSmall.Operation!.Id, Is.EqualTo(operationId));
            });
        }

        await using (var projectionContext = database.CreateContext())
        {
            projectionContext.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    organization.BillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_checkout_revalidated",
                        "sub_checkout_revalidated",
                        "price_checkout_revalidated",
                        CommercialSubscriptionStatus.Active,
                        2,
                        false,
                        SignupTime,
                        SignupTime.AddMonths(1),
                        CheckoutTime.AddMinutes(3))));
            await projectionContext.SaveChangesAsync();
        }

        await using var subscriptionContext = database.CreateContext();
        var subscribed = await new PostgresBillingCheckoutStore(subscriptionContext.CreateContextFactory()).PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            BillingCadence.Monthly,
            1,
            operationId,
            CheckoutTime.AddMinutes(4),
            CancellationToken.None);
        Assert.That(
            subscribed.Status,
            Is.EqualTo(BillingCheckoutPreparationStatus.SubscriptionAlreadyExists));
    }

    [Test]
    public async Task ElapsedCheckoutRequiresProviderReconciliationBeforeReplacement()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-expiry",
            "checkout-expiry@example.com");
        Guid firstOperationId;
        await using (var firstContext = database.CreateContext())
        {
            var first = await new PostgresBillingCheckoutStore(firstContext.CreateContextFactory()).PrepareAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                retryOperationId: null,
                CheckoutTime,
                CancellationToken.None);
            firstOperationId = first.Operation!.Id;
        }

        await using (var replacementContext = database.CreateContext())
        {
            var reconciliation = await new PostgresBillingCheckoutStore(replacementContext.CreateContextFactory())
                .PrepareAsync(
                    signup.UserId,
                    signup.PersonalBillingAccountId,
                    BillingCadence.Annual,
                    1,
                    retryOperationId: null,
                    CheckoutTime.Add(BillingOperation.CheckoutLifetime),
                    CancellationToken.None);
            Assert.Multiple(() =>
            {
                Assert.That(
                    reconciliation.Status,
                    Is.EqualTo(
                        BillingCheckoutPreparationStatus.ProviderSessionReconciliationRequired));
                Assert.That(reconciliation.Operation!.Id, Is.EqualTo(firstOperationId));
            });
        }

        await using var verification = database.CreateContext();
        var operations = await verification.BillingOperations
            .OrderBy(operation => operation.CreatedAt)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(operations, Has.Length.EqualTo(1));
            Assert.That(operations[0].Status, Is.EqualTo(BillingOperationStatus.Pending));
            Assert.That(operations[0].ClosedAt, Is.Null);
        });
    }

    [Test]
    public async Task ProviderSessionRecordingPersistsAndIsSafelyRepeatable()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-complete",
            "checkout-complete@example.com");
        Guid operationId;
        await using (var context = database.CreateContext())
        {
            var prepared = await new PostgresBillingCheckoutStore(context.CreateContextFactory()).PrepareAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                seatQuantity: 1,
                retryOperationId: null,
                CheckoutTime,
                CancellationToken.None);
            operationId = prepared.Operation!.Id;
        }

        await using (var context = database.CreateContext())
        {
            var store = new PostgresBillingCheckoutStore(context.CreateContextFactory());
            Assert.That(
                await store.RecordProviderSessionAsync(
                    operationId,
                    "cs_completed",
                    CheckoutTime.AddMinutes(1),
                    CancellationToken.None),
                Is.True);
            Assert.That(
                await store.RecordProviderSessionAsync(
                    operationId,
                    "cs_completed",
                    CheckoutTime.AddMinutes(2),
                    CancellationToken.None),
                Is.True);
        }

        await using var verification = database.CreateContext();
        var operation = await verification.BillingOperations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                operation.Status,
                Is.EqualTo(BillingOperationStatus.ProviderSessionCreated));
            Assert.That(operation.ExternalSessionId, Is.EqualTo("cs_completed"));
            Assert.That(
                operation.ProviderSessionRecordedAt,
                Is.EqualTo(CheckoutTime.AddMinutes(1)));
        });
    }

    [Test]
    public async Task ConcurrentProviderSessionRecordingSerializesOnBillingAccount()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-completion-race",
            "checkout-completion-race@example.com");
        Guid operationId;
        await using (var setupContext = database.CreateContext())
        {
            operationId = (await new PostgresBillingCheckoutStore(setupContext.CreateContextFactory()).PrepareAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                retryOperationId: null,
                CheckoutTime,
                CancellationToken.None)).Operation!.Id;
        }

        var firstAccountLockGate = new DatabaseCommandGate();
        var secondAccountLockGate = new DatabaseCommandGate();
        await using var firstContext = database.CreateContext(
            new DatabaseCommandGateInterceptor(
                firstAccountLockGate,
                "actor_user_id"));
        await using var secondContext = database.CreateContext(
            new DatabaseCommandGateInterceptor(
                secondAccountLockGate,
                "UPDATE billing_accounts"));
        var completedAt = CheckoutTime.AddMinutes(1);

        var firstTask = new PostgresBillingCheckoutStore(firstContext.CreateContextFactory())
            .RecordProviderSessionAsync(
                operationId,
                "cs_completion_race",
                completedAt,
                CancellationToken.None);
        bool[] results;
        try
        {
            await firstAccountLockGate.WaitUntilReachedAsync();
            var secondTask = new PostgresBillingCheckoutStore(secondContext.CreateContextFactory())
                .RecordProviderSessionAsync(
                operationId,
                "cs_completion_race",
                completedAt,
                CancellationToken.None);
            await secondAccountLockGate.WaitUntilReachedAsync();
            secondAccountLockGate.Release();
            firstAccountLockGate.Release();
            results = await Task.WhenAll(firstTask, secondTask);
        }
        finally
        {
            secondAccountLockGate.Release();
            firstAccountLockGate.Release();
        }

        await using var verification = database.CreateContext();
        var operation = await verification.BillingOperations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(results, Is.All.True);
            Assert.That(
                operation.Status,
                Is.EqualTo(BillingOperationStatus.ProviderSessionCreated));
            Assert.That(operation.ExternalSessionId, Is.EqualTo("cs_completion_race"));
            Assert.That(operation.ProviderSessionRecordedAt, Is.EqualTo(completedAt));
        });
    }

    [Test]
    public async Task ProviderConfirmedExpiryClosesRecordedSessionUnderAccountLock()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "checkout-provider-expiry",
            "checkout-provider-expiry@example.com");
        Guid operationId;
        await using (var context = database.CreateContext())
        {
            var store = new PostgresBillingCheckoutStore(context.CreateContextFactory());
            operationId = (await store.PrepareAsync(
                signup.UserId,
                signup.PersonalBillingAccountId,
                BillingCadence.Monthly,
                1,
                retryOperationId: null,
                CheckoutTime,
                CancellationToken.None)).Operation!.Id;
            await store.RecordProviderSessionAsync(
                operationId,
                "cs_provider_expired",
                CheckoutTime.AddMinutes(1),
                CancellationToken.None);
        }

        var expiredAt = CheckoutTime.Add(BillingOperation.CheckoutLifetime);
        await using (var context = database.CreateContext())
        {
            Assert.That(
                await new PostgresBillingCheckoutStore(context.CreateContextFactory()).ExpireProviderSessionAsync(
                    operationId,
                    "cs_provider_expired",
                    expiredAt,
                    CancellationToken.None),
                Is.True);
        }

        await using var verification = database.CreateContext();
        var operation = await verification.BillingOperations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Expired));
            Assert.That(operation.ClosedAt, Is.EqualTo(expiredAt));
            Assert.That(operation.ExternalSessionId, Is.EqualTo("cs_provider_expired"));
        });
    }

    [Test]
    public async Task CheckoutProjectionAndCompletionRollbackAndRetryAtomically()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "checkout-atomic-completion",
            "checkout-atomic-completion@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Atomic Checkout completion",
            SignupTime.AddDays(1));
        Guid operationId;
        await using (var setupContext = database.CreateContext())
        {
            var store = new PostgresBillingCheckoutStore(setupContext.CreateContextFactory());
            var prepared = await store.PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                BillingCadence.Monthly,
                1,
                null,
                CheckoutTime,
                CancellationToken.None);
            operationId = prepared.Operation!.Id;
            Assert.That(
                await store.RecordProviderSessionAsync(
                    operationId,
                    "cs_atomic_completion",
                    CheckoutTime.AddMinutes(1),
                    CancellationToken.None),
                Is.True);
        }

        var projectedAt = CheckoutTime.AddMinutes(2);
        var subscription = new AuthoritativeCommercialSubscription(
            organization.BillingAccountId,
            new CommercialSubscriptionProjection(
                "cus_atomic_completion",
                "sub_atomic_completion",
                "price_test_monthly",
                CommercialSubscriptionStatus.Active,
                seatQuantity: 1,
                cancelAtPeriodEnd: false,
                CheckoutTime,
                CheckoutTime.AddMonths(1),
                projectedAt));
        await using (var failingContext = database.CreateContext(
            new ThrowAfterSaveInterceptor()))
        {
            Assert.ThrowsAsync<SimulatedPostSaveException>(
                async () => await new PostgresBillingCheckoutStore(failingContext.CreateContextFactory())
                    .ApplySubscriptionAndCompleteProviderSessionAsync(
                        operationId,
                        subscription,
                        CancellationToken.None));
        }

        await using (var rolledBack = database.CreateContext())
        {
            Assert.Multiple(() =>
            {
                Assert.That(rolledBack.CommercialSubscriptions, Is.Empty);
                Assert.That(
                    rolledBack.BillingOperations.Single().Status,
                    Is.EqualTo(BillingOperationStatus.ProviderSessionCreated));
            });
        }

        await using (var retryContext = database.CreateContext())
        {
            Assert.That(
                await new PostgresBillingCheckoutStore(retryContext.CreateContextFactory())
                    .ApplySubscriptionAndCompleteProviderSessionAsync(
                        operationId,
                        subscription,
                        CancellationToken.None),
                Is.True);
        }

        await using var seatContext = database.CreateContext();
        var seatChange = await new PostgresBillingSeatQuantityStore(seatContext.CreateContextFactory())
            .PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                2,
                null,
                projectedAt.AddMinutes(1),
                CancellationToken.None);
        Assert.Multiple(() =>
        {
            Assert.That(seatChange.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.Prepared));
            Assert.That(
                seatContext.BillingOperations.Count(
                    operation => operation.Status == BillingOperationStatus.Completed),
                Is.EqualTo(1));
            Assert.That(seatContext.CommercialSubscriptions.Single().SeatQuantity, Is.EqualTo(1));
        });
    }
}
