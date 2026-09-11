using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class BillingSeatQuantityPersistenceTests
{
    private static readonly DateTimeOffset ChangeTime = SignupTime.AddDays(4);

    [Test]
    public async Task OrganizationOwnerStartsAnUnboundedDurableChangeWithAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-unbounded-owner",
            "seat-change-unbounded-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Unbounded seat change",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);

        await using (var context = database.CreateContext())
        {
            var result = await new PostgresBillingSeatQuantityStore(context.CreateContextFactory()).PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                int.MaxValue,
                retryOperationId: null,
                ChangeTime,
                CancellationToken.None);

            Assert.Multiple(() =>
            {
                Assert.That(result.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.Prepared));
                Assert.That(result.RequiredSeatQuantity, Is.EqualTo(1));
                Assert.That(
                    result.ExternalSubscriptionId,
                    Is.EqualTo($"sub_{organization.BillingAccountId:N}"));
                Assert.That(result.Operation!.Id.Version, Is.EqualTo(7));
                Assert.That(result.Operation.PreviousSeatQuantity, Is.EqualTo(2));
                Assert.That(result.Operation.SeatQuantity, Is.EqualTo(int.MaxValue));
                Assert.That(result.Operation.ExpiresAt, Is.Null);
            });
        }

        await using var verification = database.CreateContext();
        var operation = await verification.BillingOperations.SingleAsync();
        var audit = await verification.AuditRecords.SingleAsync(
            record => record.Action == AuditAction.BillingSeatQuantityChangeStarted);
        Assert.Multiple(() =>
        {
            Assert.That(operation.Kind, Is.EqualTo(BillingOperationKind.SeatQuantityChange));
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Pending));
            Assert.That(operation.Cadence, Is.Null);
            Assert.That(audit.CorrelationId, Is.EqualTo(operation.Id));
            Assert.That(audit.TargetId, Is.EqualTo(operation.Id));
            Assert.That(audit.ActorUserId, Is.EqualTo(owner.UserId));
        });
    }

    [Test]
    public async Task DecreaseMustCoverAssignedSeatsAndActiveInvitations()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-capacity-owner",
            "seat-change-capacity-owner@example.com");
        var member = await SignUpAsync(
            database,
            "seat-change-capacity-member",
            "seat-change-capacity-member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat change capacity",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 5);
        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "seat-change-pending@example.com",
            ChangeTime.AddHours(-1));
        await using var context = database.CreateContext();
        var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());

        var tooSmall = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            seatQuantity: 2,
            retryOperationId: null,
            ChangeTime,
            CancellationToken.None);
        var prepared = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            seatQuantity: 3,
            retryOperationId: null,
            ChangeTime,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(tooSmall.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.SeatQuantityTooSmall));
            Assert.That(tooSmall.RequiredSeatQuantity, Is.EqualTo(3));
            Assert.That(prepared.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.Prepared));
            Assert.That(prepared.RequiredSeatQuantity, Is.EqualTo(3));
        });
    }

    [Test]
    public async Task AuthorizationSubscriptionAndEligibilityFailClosed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-auth-owner",
            "seat-change-auth-owner@example.com");
        var administrator = await SignUpAsync(
            database,
            "seat-change-auth-admin",
            "seat-change-auth-admin@example.com");
        var outsider = await SignUpAsync(
            database,
            "seat-change-auth-outsider",
            "seat-change-auth-outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat change authorization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            administrator.UserId,
            OrganizationRole.Admin);
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 4);
        await ProjectSubscriptionAsync(
            database,
            outsider.PersonalBillingAccountId,
            1,
            CommercialSubscriptionStatus.Unpaid);
        await using var context = database.CreateContext();
        var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());

        var administratorResult = await store.PrepareAsync(
            administrator.UserId,
            organization.BillingAccountId,
            5,
            null,
            ChangeTime,
            CancellationToken.None);
        var outsiderResult = await store.PrepareAsync(
            outsider.UserId,
            organization.BillingAccountId,
            5,
            null,
            ChangeTime,
            CancellationToken.None);
        var missingSubscription = await store.PrepareAsync(
            owner.UserId,
            owner.PersonalBillingAccountId,
            1,
            null,
            ChangeTime,
            CancellationToken.None);
        var inactiveSubscription = await store.PrepareAsync(
            outsider.UserId,
            outsider.PersonalBillingAccountId,
            1,
            null,
            ChangeTime,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(administratorResult.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.InsufficientPermission));
            Assert.That(outsiderResult.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.BillingAccountNotFound));
            Assert.That(missingSubscription.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.SubscriptionNotFound));
            Assert.That(inactiveSubscription.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.SubscriptionInactive));
            Assert.That(context.BillingOperations, Is.Empty);
        });
    }

    [Test]
    public async Task PersonalQuantityIsFixedAndAnUnchangedTargetIsRejected()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-personal",
            "seat-change-personal@example.com");
        await ProjectSubscriptionAsync(database, owner.PersonalBillingAccountId, 1);
        await using var context = database.CreateContext();
        var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());

        var invalid = await store.PrepareAsync(
            owner.UserId,
            owner.PersonalBillingAccountId,
            2,
            null,
            ChangeTime,
            CancellationToken.None);
        var unchanged = await store.PrepareAsync(
            owner.UserId,
            owner.PersonalBillingAccountId,
            1,
            null,
            ChangeTime,
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(invalid.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.PersonalSeatQuantityInvalid));
            Assert.That(invalid.RequiredSeatQuantity, Is.EqualTo(1));
            Assert.That(unchanged.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.SeatQuantityUnchanged));
        });
    }

    [Test]
    public async Task IdenticalRequestRecoversOperationAndChangedRequestReturnsItsSafeState()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-retry-owner",
            "seat-change-retry-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat change retry",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        await using var context = database.CreateContext();
        var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());

        var started = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            8,
            null,
            ChangeTime,
            CancellationToken.None);
        var recovered = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            8,
            null,
            ChangeTime.AddMinutes(1),
            CancellationToken.None);
        var inProgress = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            9,
            null,
            ChangeTime.AddMinutes(1),
            CancellationToken.None);
        var wrongRetry = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            8,
            Guid.CreateVersion7(),
            ChangeTime.AddMinutes(1),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(recovered.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.Prepared));
            Assert.That(recovered.Operation!.Id, Is.EqualTo(started.Operation!.Id));
            Assert.That(inProgress.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.OperationInProgress));
            Assert.That(inProgress.Operation!.Id, Is.EqualTo(started.Operation.Id));
            Assert.That(inProgress.Operation.PreviousSeatQuantity, Is.EqualTo(2));
            Assert.That(inProgress.Operation.SeatQuantity, Is.EqualTo(8));
            Assert.That(wrongRetry.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.BillingOperationNotFound));
            Assert.That(context.BillingOperations.Count(), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task CurrentOwnerCanRecoverAnOperationStartedBeforeOwnershipTransfer()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var originalOwner = await SignUpAsync(
            database,
            "seat-change-transfer-original",
            "seat-change-transfer-original@example.com");
        var newOwner = await SignUpAsync(
            database,
            "seat-change-transfer-new",
            "seat-change-transfer-new@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            originalOwner.UserId,
            "Transferred seat change",
            SignupTime.AddDays(1));
        var newOwnerMembership = await AddOrganizationMemberAsync(
            database,
            organization,
            newOwner.UserId,
            OrganizationRole.Member);
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        Guid operationId;
        await using (var initialContext = database.CreateContext())
        {
            var initial = await new PostgresBillingSeatQuantityStore(initialContext.CreateContextFactory())
                .PrepareAsync(
                    originalOwner.UserId,
                    organization.BillingAccountId,
                    3,
                    null,
                    ChangeTime,
                    CancellationToken.None);
            operationId = initial.Operation!.Id;
        }

        await using (var transferContext = database.CreateContext())
        {
            await using var roleTest = ServiceTestBase<OrganizationRoleManagementService>.ForDatabase(
                database, ChangeTime.AddMinutes(1));
            var transfer = await roleTest.Service
                .TransferOwnershipAsync(
                    originalOwner.UserId,
                    organization.OrganizationId,
                    newOwnerMembership.MembershipId);
            Assert.That(
                transfer.Status,
                Is.EqualTo(OrganizationRoleManagementStatus.OwnershipTransferred));
        }

        await using var retryContext = database.CreateContext();
        var store = new PostgresBillingSeatQuantityStore(retryContext.CreateContextFactory());
        var recovered = await store.PrepareAsync(
            newOwner.UserId,
            organization.BillingAccountId,
            3,
            operationId,
            ChangeTime.AddMinutes(2),
            CancellationToken.None);
        var previousOwner = await store.PrepareAsync(
            originalOwner.UserId,
            organization.BillingAccountId,
            3,
            operationId,
            ChangeTime.AddMinutes(2),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(recovered.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.Prepared));
            Assert.That(recovered.Operation!.Id, Is.EqualTo(operationId));
            Assert.That(recovered.Operation.ActorUserId, Is.EqualTo(originalOwner.UserId));
            Assert.That(previousOwner.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.InsufficientPermission));
        });
    }

    [Test]
    public async Task LiveOperationCanReconcileAfterSubscriptionBecomesInactive()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-inactive-recovery",
            "seat-change-inactive-recovery@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Inactive seat recovery",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        Guid operationId;
        await using (var initialContext = database.CreateContext())
        {
            var initial = await new PostgresBillingSeatQuantityStore(initialContext.CreateContextFactory())
                .PrepareAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    3,
                    null,
                    ChangeTime,
                    CancellationToken.None);
            operationId = initial.Operation!.Id;
        }

        await ProjectSubscriptionAsync(
            database,
            organization.BillingAccountId,
            2,
            CommercialSubscriptionStatus.Unpaid,
            ChangeTime.AddMinutes(1));
        await using var retryContext = database.CreateContext();
        var store = new PostgresBillingSeatQuantityStore(retryContext.CreateContextFactory());
        var recovered = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            3,
            operationId,
            ChangeTime.AddMinutes(2),
            CancellationToken.None);
        Assert.That(recovered.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.Prepared));

        Assert.That(
            await store.RejectProviderMutationAsync(
                operationId,
                ChangeTime.AddMinutes(3),
                CancellationToken.None),
            Is.Not.Null);
        var fresh = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            4,
            null,
            ChangeTime.AddMinutes(4),
            CancellationToken.None);
        Assert.That(fresh.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.SubscriptionInactive));
    }

    [TestCase(true)]
    [TestCase(false)]
    public async Task ConcurrentRequestsCreateOneDurableOperation(bool identicalTargets)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"seat-change-race-{identicalTargets}",
            $"seat-change-race-{identicalTargets}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Concurrent seat change",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        await using var firstContext = database.CreateContext(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE billing_accounts"));
        await using var secondContext = database.CreateContext(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE billing_accounts"));

        var results = await Task.WhenAll(
            new PostgresBillingSeatQuantityStore(firstContext.CreateContextFactory()).PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                5,
                null,
                ChangeTime,
                CancellationToken.None),
            new PostgresBillingSeatQuantityStore(secondContext.CreateContextFactory()).PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                identicalTargets ? 5 : 6,
                null,
                ChangeTime,
                CancellationToken.None));

        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(
                results.Select(result => result.Operation!.Id).Distinct().Count(),
                Is.EqualTo(1));
            Assert.That(
                results.Select(result => result.Status),
                identicalTargets
                    ? Has.All.EqualTo(BillingSeatQuantityPreparationStatus.Prepared)
                    : Is.EquivalentTo(new[]
                    {
                        BillingSeatQuantityPreparationStatus.Prepared,
                        BillingSeatQuantityPreparationStatus.OperationInProgress,
                    }));
            Assert.That(verification.BillingOperations.Count(), Is.EqualTo(1));
            Assert.That(
                verification.AuditRecords.Count(
                    record => record.Action == AuditAction.BillingSeatQuantityChangeStarted),
                Is.EqualTo(1));
        });
    }

    [Test]
    public async Task ConcurrentIdenticalRetriesCloseOneOperationWithoutAConcurrencyFailure()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-close-race",
            "seat-change-close-race@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Concurrent seat closure",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);

        Guid operationId;
        await using (var preparationContext = database.CreateContext())
        {
            var prepared = await new PostgresBillingSeatQuantityStore(preparationContext.CreateContextFactory())
                .PrepareAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    retryOperationId: null,
                    ChangeTime,
                    CancellationToken.None);
            operationId = prepared.Operation!.Id;
        }

        var provider = new ConcurrentTargetSeatQuantityProvider(
            organization.BillingAccountId,
            targetQuantity: 5);
        await using var billingTest = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
            database, ChangeTime.AddMinutes(1), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
        await using var billingTest2 = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
            database, ChangeTime.AddMinutes(2), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
        var results = await Task.WhenAll(
            billingTest.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    operationId,
                    CancellationToken.None),
            billingTest2.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    operationId,
                    CancellationToken.None));

        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                results.Select(result => result.Status),
                Has.All.EqualTo(BillingSeatQuantityStatus.Changed));
            Assert.That(provider.ObservationCount, Is.EqualTo(2));
            Assert.That(
                verification.BillingOperations.Single().Status,
                Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(
                verification.CommercialSubscriptions.Single().SeatQuantity,
                Is.EqualTo(5));
        });
    }

    [Test]
    public async Task ConcurrentMutationRequiredRetriesShareOneIdempotentMutationAndConverge()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-mutation-race",
            "seat-change-mutation-race@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Concurrent seat mutation",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);

        Guid operationId;
        await using (var preparationContext = database.CreateContext())
        {
            var prepared = await new PostgresBillingSeatQuantityStore(preparationContext.CreateContextFactory())
                .PrepareAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    retryOperationId: null,
                    ChangeTime,
                    CancellationToken.None);
            operationId = prepared.Operation!.Id;
        }

        var provider = new ConcurrentIdempotentSeatQuantityProvider(
            organization.BillingAccountId,
            previousQuantity: 2,
            targetQuantity: 5);
        await using var billingTest = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
            database, ChangeTime.AddMinutes(1), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
        await using var billingTest2 = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
            database, ChangeTime.AddMinutes(2), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
        var results = await Task.WhenAll(
            billingTest.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    operationId,
                    CancellationToken.None),
            billingTest2.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    operationId,
                    CancellationToken.None));

        BillingSeatQuantityResult terminalRetry;
        await using (var retryContext = database.CreateContext())
        {
            await using var billingTest3 = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime.AddMinutes(3), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
            terminalRetry = await billingTest3.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    operationId,
                    CancellationToken.None);
        }

        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                results.Select(result => result.Status),
                Has.All.EqualTo(BillingSeatQuantityStatus.Changed));
            Assert.That(terminalRetry.Status, Is.EqualTo(BillingSeatQuantityStatus.Changed));
            Assert.That(terminalRetry.AuthoritativeSeatQuantity, Is.EqualTo(5));
            Assert.That(provider.ObservationCount, Is.EqualTo(2));
            Assert.That(provider.ApplyCount, Is.EqualTo(1));
            Assert.That(provider.MutationCount, Is.EqualTo(1));
            Assert.That(
                verification.BillingOperations.Single().Status,
                Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(
                verification.CommercialSubscriptions.Single().SeatQuantity,
                Is.EqualTo(5));
        });
    }

    [TestCase(
        BillingSeatQuantityProviderStatus.Superseded,
        6,
        CommercialSubscriptionStatus.Active)]
    [TestCase(
        BillingSeatQuantityProviderStatus.Applied,
        5,
        CommercialSubscriptionStatus.PastDue)]
    public async Task TerminalClosureAppliesANewerProjectionAndReturnsTheDurableOutcome(
        BillingSeatQuantityProviderStatus laterProviderStatus,
        int laterSeatQuantity,
        CommercialSubscriptionStatus laterSubscriptionStatus)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"seat-change-terminal-projection-{laterProviderStatus}",
            $"seat-change-terminal-projection-{laterProviderStatus}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Terminal seat projection",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);

        Guid operationId;
        await using (var preparationContext = database.CreateContext())
        {
            var prepared = await new PostgresBillingSeatQuantityStore(preparationContext.CreateContextFactory())
                .PrepareAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    retryOperationId: null,
                    ChangeTime,
                    CancellationToken.None);
            operationId = prepared.Operation!.Id;
        }

        BillingSeatQuantityResolutionResult? firstClosure;
        await using (var firstContext = database.CreateContext())
        {
            firstClosure = await new PostgresBillingSeatQuantityStore(firstContext.CreateContextFactory())
                .ApplySubscriptionAndResolveAsync(
                    operationId,
                    AuthoritativeSubscription(
                        organization.BillingAccountId,
                        seatQuantity: 5,
                        CommercialSubscriptionStatus.Active,
                        ChangeTime.AddMinutes(1)),
                    ChangeTime.AddMinutes(1),
                    CancellationToken.None);
        }

        BillingSeatQuantityResolutionResult? laterClosure;
        await using (var laterContext = database.CreateContext())
        {
            laterClosure = await new PostgresBillingSeatQuantityStore(laterContext.CreateContextFactory())
                .ApplySubscriptionAndResolveAsync(
                    operationId,
                    AuthoritativeSubscription(
                        organization.BillingAccountId,
                        laterSeatQuantity,
                        laterSubscriptionStatus,
                        ChangeTime.AddMinutes(2)),
                    ChangeTime.AddMinutes(2),
                    CancellationToken.None);
        }

        await using var verification = database.CreateContext();
        var subscription = verification.CommercialSubscriptions.Single();
        Assert.Multiple(() =>
        {
            Assert.That(
                firstClosure!.OperationStatus,
                Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(firstClosure.AuthoritativeSeatQuantity, Is.EqualTo(5));
            Assert.That(
                laterClosure!.OperationStatus,
                Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(
                laterClosure.AuthoritativeSeatQuantity,
                Is.EqualTo(laterSeatQuantity));
            Assert.That(
                verification.BillingOperations.Single().Status,
                Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(subscription.SeatQuantity, Is.EqualTo(laterSeatQuantity));
            Assert.That(subscription.Status, Is.EqualTo(laterSubscriptionStatus));
        });
    }

    [Test]
    public async Task StaleMutationObservationClosesAgainstNewerPersistedTarget()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-stale-observation-target",
            "seat-change-stale-observation-target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Stale observation target",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);

        await using var context = database.CreateContext();
        var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());
        var prepared = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            5,
            retryOperationId: null,
            ChangeTime,
            CancellationToken.None);
        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, ChangeTime);
        await projectionTest.Service
            .ApplyAsync(
                AuthoritativeSubscription(
                    organization.BillingAccountId,
                    seatQuantity: 5,
                    CommercialSubscriptionStatus.Active,
                    projectedAt: ChangeTime.AddHours(2),
                    providerReadRevision: 2));

        var result = await store.ApplyObservationAndPrepareProviderMutationAsync(
            prepared.Operation!.Id,
            AuthoritativeSubscription(
                organization.BillingAccountId,
                seatQuantity: 2,
                CommercialSubscriptionStatus.Active,
                projectedAt: ChangeTime.AddHours(1),
                providerReadRevision: 1),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result!.OperationStatus, Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(result.Outcome, Is.EqualTo(SeatQuantityChangeOutcome.Applied));
            Assert.That(result.AuthoritativeSeatQuantity, Is.EqualTo(5));
            Assert.That(result.ProviderMutationReplayStartedAt, Is.Null);
            Assert.That(
                context.CommercialSubscriptions.Single().ProviderReadRevision,
                Is.EqualTo(2));
        });
    }

    [Test]
    public async Task ChangedAuthorizedObservationCannotStartAProviderMutation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-exact-observation-fence",
            "seat-change-exact-observation-fence@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Exact observation fence",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 3);

        Guid operationId;
        var authorizedObservation = AuthoritativeSubscription(
            organization.BillingAccountId,
            seatQuantity: 3,
            CommercialSubscriptionStatus.Active,
            projectedAt: ChangeTime.AddMinutes(1),
            providerReadRevision: 1);
        await using (var authorizationContext = database.CreateContext())
        {
            var store = new PostgresBillingSeatQuantityStore(authorizationContext.CreateContextFactory());
            var prepared = await store.PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                seatQuantity: 5,
                retryOperationId: null,
                ChangeTime,
                CancellationToken.None);
            operationId = prepared.Operation!.Id;
            var authorization = await store.ApplyObservationAndPrepareProviderMutationAsync(
                operationId,
                authorizedObservation,
                CancellationToken.None);
            Assert.That(
                authorization!.OperationStatus,
                Is.EqualTo(BillingOperationStatus.Pending));
        }

        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, ChangeTime))
        {
            await projectionTest.Service
                .ApplyAsync(
                    AuthoritativeSubscription(
                        organization.BillingAccountId,
                        seatQuantity: 6,
                        CommercialSubscriptionStatus.Active,
                        projectedAt: ChangeTime.AddMinutes(2),
                        providerReadRevision: 2));
        }

        var mutationStarted = false;
        BillingSeatQuantityResolutionResult? resolution;
        await using (var mutationContext = database.CreateContext())
        {
            resolution = await new PostgresBillingSeatQuantityStore(mutationContext.CreateContextFactory())
                .ApplyProviderMutationIfObservationCurrentAsync(
                    operationId,
                    authorizedObservation,
                    _ =>
                    {
                        mutationStarted = true;
                        return Task.FromResult(
                            AuthoritativeSubscription(
                                organization.BillingAccountId,
                                seatQuantity: 5,
                                CommercialSubscriptionStatus.Active,
                                projectedAt: ChangeTime.AddMinutes(3),
                                providerReadRevision: 3));
                    },
                    ChangeTime.AddMinutes(3),
                    CancellationToken.None);
        }

        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(mutationStarted, Is.False);
            Assert.That(resolution!.OperationStatus, Is.EqualTo(BillingOperationStatus.Failed));
            Assert.That(resolution.Outcome, Is.EqualTo(SeatQuantityChangeOutcome.Superseded));
            Assert.That(resolution.AuthoritativeSeatQuantity, Is.EqualTo(6));
            Assert.That(verification.CommercialSubscriptions.Single().SeatQuantity, Is.EqualTo(6));
        });
    }

    [Test]
    public async Task StaleAppliedResponseClosesAgainstNewerSupersedingQuantity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-stale-applied-superseded",
            "seat-change-stale-applied-superseded@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Stale applied superseded",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);

        await using var context = database.CreateContext();
        var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());
        var prepared = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            5,
            retryOperationId: null,
            ChangeTime,
            CancellationToken.None);
        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, ChangeTime);
        await projectionTest.Service
            .ApplyAsync(
                AuthoritativeSubscription(
                    organization.BillingAccountId,
                    seatQuantity: 6,
                    CommercialSubscriptionStatus.Active,
                    projectedAt: ChangeTime.AddHours(2),
                    providerReadRevision: 2));

        var result = await store.ApplySubscriptionAndResolveAsync(
            prepared.Operation!.Id,
            AuthoritativeSubscription(
                organization.BillingAccountId,
                seatQuantity: 5,
                CommercialSubscriptionStatus.Active,
                projectedAt: ChangeTime.AddHours(1),
                providerReadRevision: 1),
            ChangeTime.AddHours(1),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(result!.OperationStatus, Is.EqualTo(BillingOperationStatus.Failed));
            Assert.That(result.Outcome, Is.EqualTo(SeatQuantityChangeOutcome.Superseded));
            Assert.That(result.AuthoritativeSeatQuantity, Is.EqualTo(6));
            Assert.That(context.CommercialSubscriptions.Single().SeatQuantity, Is.EqualTo(6));
        });
    }

    [Test]
    public async Task NewerProviderResponseWinsEvenWithLowerReadRevision()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-provider-fence",
            "seat-change-provider-fence@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat mutation fence",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);

        await using var context = database.CreateContext();
        var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());
        var prepared = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            5,
            retryOperationId: null,
            ChangeTime,
            CancellationToken.None);
        var operationId = prepared.Operation!.Id;
        var preUpdate = await store.ApplyObservationAndPrepareProviderMutationAsync(
            operationId,
            AuthoritativeSubscription(
                organization.BillingAccountId,
                seatQuantity: 2,
                CommercialSubscriptionStatus.Active,
                projectedAt: ChangeTime.AddHours(1),
                providerReadRevision: 3),
            CancellationToken.None);
        var completed = await store.ApplySubscriptionAndResolveAsync(
            operationId,
            AuthoritativeSubscription(
                organization.BillingAccountId,
                seatQuantity: 5,
                CommercialSubscriptionStatus.Active,
                projectedAt: ChangeTime.AddHours(2),
                providerReadRevision: 2),
            ChangeTime.AddMinutes(1),
            CancellationToken.None);

        Assert.Multiple(() =>
        {
            Assert.That(preUpdate!.AuthoritativeSeatQuantity, Is.EqualTo(2));
            Assert.That(
                completed!.OperationStatus,
                Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(completed.AuthoritativeSeatQuantity, Is.EqualTo(5));
            Assert.That(context.CommercialSubscriptions.Single().SeatQuantity, Is.EqualTo(5));
            Assert.That(
                context.CommercialSubscriptions.Single().ProviderReadRevision,
                Is.EqualTo(2));
        });
    }

    [Test]
    public async Task FailedTerminalClosureKeepsItsOutcomeWhileApplyingNewerProjection()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-failed-terminal-projection",
            "seat-change-failed-terminal-projection@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Failed terminal seat projection",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);

        Guid operationId;
        await using (var preparationContext = database.CreateContext())
        {
            var prepared = await new PostgresBillingSeatQuantityStore(preparationContext.CreateContextFactory())
                .PrepareAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    retryOperationId: null,
                    ChangeTime,
                    CancellationToken.None);
            operationId = prepared.Operation!.Id;
        }

        await using (var firstContext = database.CreateContext())
        {
            var failed = await new PostgresBillingSeatQuantityStore(firstContext.CreateContextFactory())
                .ApplySubscriptionAndResolveAsync(
                    operationId,
                    AuthoritativeSubscription(
                        organization.BillingAccountId,
                        seatQuantity: 4,
                        CommercialSubscriptionStatus.Active,
                        ChangeTime.AddMinutes(1)),
                    ChangeTime.AddMinutes(1),
                    CancellationToken.None);
            Assert.Multiple(() =>
            {
                Assert.That(
                    failed!.OperationStatus,
                    Is.EqualTo(BillingOperationStatus.Failed));
                Assert.That(
                    failed.Outcome,
                    Is.EqualTo(SeatQuantityChangeOutcome.Superseded));
                Assert.That(failed.AuthoritativeSeatQuantity, Is.EqualTo(4));
            });
        }

        await using (var laterContext = database.CreateContext())
        {
            var later = await new PostgresBillingSeatQuantityStore(laterContext.CreateContextFactory())
                .ApplySubscriptionAndResolveAsync(
                    operationId,
                    AuthoritativeSubscription(
                        organization.BillingAccountId,
                        seatQuantity: 5,
                        CommercialSubscriptionStatus.Active,
                        ChangeTime.AddMinutes(2)),
                    ChangeTime.AddMinutes(2),
                    CancellationToken.None);
            Assert.Multiple(() =>
            {
                Assert.That(
                    later!.OperationStatus,
                    Is.EqualTo(BillingOperationStatus.Failed));
                Assert.That(
                    later.Outcome,
                    Is.EqualTo(SeatQuantityChangeOutcome.Superseded));
                Assert.That(later.AuthoritativeSeatQuantity, Is.EqualTo(5));
            });
        }

        BillingSeatQuantityResult retry;
        await using (var retryContext = database.CreateContext())
        {
            await using var billingTest = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime.AddMinutes(3), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(new RejectingSeatQuantityProvider()); });
            retry = await billingTest.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    operationId,
                    CancellationToken.None);
        }

        await using var verification = database.CreateContext();
        var operation = verification.BillingOperations.Single();
        Assert.Multiple(() =>
        {
            Assert.That(
                retry.Status,
                Is.EqualTo(BillingSeatQuantityStatus.SubscriptionQuantityChanged));
            Assert.That(retry.AuthoritativeSeatQuantity, Is.EqualTo(5));
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Failed));
            Assert.That(
                operation.SeatQuantityOutcome,
                Is.EqualTo(SeatQuantityChangeOutcome.Superseded));
            Assert.That(
                verification.CommercialSubscriptions.Single().SeatQuantity,
                Is.EqualTo(5));
        });
    }

    [Test]
    public async Task ProviderRejectionIsDurableAndExactRetryReproducesTheFailure()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-provider-rejected",
            "seat-change-provider-rejected@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Rejected seat change",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);

        Guid operationId;
        await using (var context = database.CreateContext())
        {
            var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());
            var prepared = await store.PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                5,
                retryOperationId: null,
                ChangeTime,
                CancellationToken.None);
            operationId = prepared.Operation!.Id;
            var rejected = await store.RejectProviderMutationAsync(
                operationId,
                ChangeTime.AddMinutes(1),
                CancellationToken.None);
            Assert.That(
                rejected!.Outcome,
                Is.EqualTo(SeatQuantityChangeOutcome.ProviderRejected));
        }

        await using (var retryContext = database.CreateContext())
        {
            await using var billingTest = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime.AddMinutes(2), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(new RejectingSeatQuantityProvider()); });
            Assert.That(
                async () => await billingTest.Service
                    .ChangeAsync(
                        owner.UserId,
                        organization.BillingAccountId,
                        5,
                        operationId,
                        CancellationToken.None),
                Throws.TypeOf<InvalidOperationException>());
        }

        await using var verification = database.CreateContext();
        var operation = verification.BillingOperations.Single();
        Assert.Multiple(() =>
        {
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Failed));
            Assert.That(
                operation.SeatQuantityOutcome,
                Is.EqualTo(SeatQuantityChangeOutcome.ProviderRejected));
        });
    }

    [TestCase(
        5,
        BillingOperationStatus.Completed,
        SeatQuantityChangeOutcome.Applied)]
    [TestCase(
        6,
        BillingOperationStatus.Failed,
        SeatQuantityChangeOutcome.Superseded)]
    public async Task ProviderRejectionDefersToNewerAuthoritativeState(
        int authoritativeQuantity,
        BillingOperationStatus expectedStatus,
        SeatQuantityChangeOutcome expectedOutcome)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"seat-change-provider-rejection-race-{authoritativeQuantity}",
            $"seat-change-provider-rejection-race-{authoritativeQuantity}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Provider rejection race",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 3);

        Guid operationId;
        await using (var operationContext = database.CreateContext())
        {
            var store = new PostgresBillingSeatQuantityStore(operationContext.CreateContextFactory());
            var prepared = await store.PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                seatQuantity: 5,
                retryOperationId: null,
                ChangeTime,
                CancellationToken.None);
            operationId = prepared.Operation!.Id;
            _ = await store.ApplyObservationAndPrepareProviderMutationAsync(
                operationId,
                AuthoritativeSubscription(
                    organization.BillingAccountId,
                    seatQuantity: 3,
                    CommercialSubscriptionStatus.Active,
                    projectedAt: ChangeTime.AddMinutes(1),
                    providerReadRevision: 1),
                CancellationToken.None);
        }

        await using (var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
                database, ChangeTime))
        {
            await projectionTest.Service
                .ApplyAsync(
                    AuthoritativeSubscription(
                        organization.BillingAccountId,
                        authoritativeQuantity,
                        CommercialSubscriptionStatus.Active,
                        projectedAt: ChangeTime.AddMinutes(2),
                        providerReadRevision: 2));
        }

        BillingSeatQuantityResolutionResult? resolution;
        await using (var rejectionContext = database.CreateContext())
        {
            resolution = await new PostgresBillingSeatQuantityStore(rejectionContext.CreateContextFactory())
                .RejectProviderMutationAsync(
                    operationId,
                    ChangeTime.AddMinutes(3),
                    CancellationToken.None);
        }

        Assert.Multiple(() =>
        {
            Assert.That(resolution!.OperationStatus, Is.EqualTo(expectedStatus));
            Assert.That(resolution.Outcome, Is.EqualTo(expectedOutcome));
            Assert.That(
                resolution.AuthoritativeSeatQuantity,
                Is.EqualTo(authoritativeQuantity));
        });
    }

    [Test]
    public async Task RetryAfterFailureBeforeProviderMutationCompletesTheSameOperation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-provider-retry",
            "seat-change-provider-retry@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Provider retry",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        var provider = new RecoveringSeatQuantityProvider(
            organization.BillingAccountId,
            currentQuantity: 2);

        BillingSeatQuantityResult initial;
        await using (var context = database.CreateContext())
        {
            await using var billingTest = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime, configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
            initial = await billingTest.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    retryOperationId: null,
                    CancellationToken.None);
        }

        BillingSeatQuantityResult recovered;
        await using (var retryContext = database.CreateContext())
        {
            await using var billingTest2 = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime.AddMinutes(1), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
            recovered = await billingTest2.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    initial.BillingOperationId,
                    CancellationToken.None);
        }

        await using var verification = database.CreateContext();
        var operation = verification.BillingOperations.Single();
        Assert.Multiple(() =>
        {
            Assert.That(initial.Status, Is.EqualTo(BillingSeatQuantityStatus.ProviderUnavailable));
            Assert.That(recovered.Status, Is.EqualTo(BillingSeatQuantityStatus.Changed));
            Assert.That(recovered.BillingOperationId, Is.EqualTo(initial.BillingOperationId));
            Assert.That(provider.ApplyCount, Is.EqualTo(2));
            Assert.That(provider.MutationCount, Is.EqualTo(1));
            Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(
                verification.CommercialSubscriptions.Single().SeatQuantity,
                Is.EqualTo(5));
        });
    }

    [Test]
    public async Task ProviderClockReplayWindowPersistsAndRetainsIndeterminateOperation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-replay-window",
            "seat-change-replay-window@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat replay window",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        var provider = new ExpiringReplaySeatQuantityProvider(
            organization.BillingAccountId,
            previousQuantity: 2,
            targetQuantity: 5)
        {
            ProviderObservedAt = ChangeTime,
        };

        BillingSeatQuantityResult first;
        await using (var firstContext = database.CreateContext())
        {
            await using var billingTest = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime.AddHours(2), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
            first = await billingTest.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    retryOperationId: null,
                    CancellationToken.None);
        }

        var operationId = first.BillingOperationId!.Value;
        provider.ProviderObservedAt = ChangeTime.AddHours(22).AddMinutes(59);
        BillingSeatQuantityResult insideWindow;
        await using (var retryContext = database.CreateContext())
        {
            await using var billingTest2 = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime.AddHours(-2), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
            insideWindow = await billingTest2.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    operationId,
                    CancellationToken.None);
        }

        provider.ProviderObservedAt = ChangeTime.AddHours(23);
        BillingSeatQuantityResult reconciliationRequired;
        await using (var expiryContext = database.CreateContext())
        {
            await using var billingTest3 = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime.AddHours(-1), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
            reconciliationRequired = await billingTest3.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    operationId,
                    CancellationToken.None);
        }

        provider.ProviderObservedAt = ChangeTime.AddHours(23).AddMinutes(1);
        BillingSeatQuantityResult implicitRetry;
        await using (var replacementContext = database.CreateContext())
        {
            await using var billingTest4 = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime.AddHours(23).AddMinutes(1), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
            implicitRetry = await billingTest4.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    retryOperationId: null,
                    CancellationToken.None);
        }

        await using var verification = database.CreateContext();
        var retainedOperation = verification.BillingOperations.Single(
            operation => operation.Id == operationId);
        Assert.Multiple(() =>
        {
            Assert.That(first.Status, Is.EqualTo(BillingSeatQuantityStatus.ProviderUnavailable));
            Assert.That(
                insideWindow.Status,
                Is.EqualTo(BillingSeatQuantityStatus.ProviderUnavailable));
            Assert.That(
                reconciliationRequired.Status,
                Is.EqualTo(BillingSeatQuantityStatus.ProviderReconciliationRequired));
            Assert.That(reconciliationRequired.AuthoritativeSeatQuantity, Is.EqualTo(2));
            Assert.That(
                implicitRetry.Status,
                Is.EqualTo(BillingSeatQuantityStatus.ProviderReconciliationRequired));
            Assert.That(implicitRetry.BillingOperationId, Is.EqualTo(operationId));
            Assert.That(provider.ObserveCount, Is.EqualTo(4));
            Assert.That(provider.ApplyCount, Is.EqualTo(2));
            Assert.That(
                retainedOperation.ProviderMutationReplayStartedAt,
                Is.EqualTo(ChangeTime));
            Assert.That(retainedOperation.SeatQuantityOutcome, Is.Null);
            Assert.That(retainedOperation.Status, Is.EqualTo(BillingOperationStatus.Pending));
            Assert.That(verification.BillingOperations.Count(), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task CompletedAndFailedOperationsReleaseTheAccountForAnotherChange()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-lifecycle-owner",
            "seat-change-lifecycle-owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat change lifecycle",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        await using var context = database.CreateContext();
        var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());
        var first = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            3,
            null,
            ChangeTime,
            CancellationToken.None);
        var firstOperation = first.Operation!;
        var observation = AuthoritativeSubscription(
            organization.BillingAccountId,
            seatQuantity: 2,
            CommercialSubscriptionStatus.Active,
            ChangeTime.AddMinutes(1));

        var mutationPreparation = await store
            .ApplyObservationAndPrepareProviderMutationAsync(
                firstOperation.Id,
                observation,
                CancellationToken.None);
        Assert.That(mutationPreparation, Is.Not.Null);
        Assert.That(
            await store.ApplySubscriptionAndResolveAsync(
                firstOperation.Id,
                AuthoritativeSubscription(
                    organization.BillingAccountId,
                    seatQuantity: 3,
                    CommercialSubscriptionStatus.Active,
                    ChangeTime.AddMinutes(2)),
                ChangeTime.AddMinutes(2),
                CancellationToken.None),
            Is.Not.Null);
        var second = await store.PrepareAsync(
            owner.UserId,
            organization.BillingAccountId,
            4,
            null,
            ChangeTime.AddMinutes(2),
            CancellationToken.None);
        Assert.That(
            await store.RejectProviderMutationAsync(
                second.Operation!.Id,
                ChangeTime.AddMinutes(3),
                CancellationToken.None),
            Is.Not.Null);

        var persistedStatuses = await context.BillingOperations.AsNoTracking()
            .ToDictionaryAsync(operation => operation.Id, operation => operation.Status);
        Assert.Multiple(() =>
        {
            Assert.That(persistedStatuses[firstOperation.Id], Is.EqualTo(BillingOperationStatus.Completed));
            Assert.That(persistedStatuses[second.Operation!.Id], Is.EqualTo(BillingOperationStatus.Failed));
            Assert.That(persistedStatuses, Has.Count.EqualTo(2));
        });
    }

    [TestCase(5, 1)]
    [TestCase(1, 5)]
    public async Task PendingChangeNeverGrantsInvitationCapacity(int currentQuantity, int target)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"seat-change-reservation-{currentQuantity}-{target}",
            $"seat-change-reservation-{currentQuantity}-{target}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seat change reservation",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, currentQuantity);
        await using (var context = database.CreateContext())
        {
            var started = await new PostgresBillingSeatQuantityStore(context.CreateContextFactory()).PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                target,
                null,
                ChangeTime,
                CancellationToken.None);
            Assert.That(started.Status, Is.EqualTo(BillingSeatQuantityPreparationStatus.Prepared));
        }

        await using var invitationTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, ChangeTime.AddMinutes(1));
        var invitation = await invitationTest.Service
            .CreateAsync(
                owner.UserId,
                organization.OrganizationId,
                "pending-change-capacity@example.com",
                OrganizationRole.Member);

        Assert.That(
            invitation.Status,
            Is.EqualTo(OrganizationInvitationCreationStatus.SeatCapacityReached));
    }

    [Test]
    public async Task FailedDecreaseRestoresPreviousInvitationCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-failed-decrease",
            "seat-change-failed-decrease@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Failed seat decrease",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        Guid operationId;
        await using (var context = database.CreateContext())
        {
            var store = new PostgresBillingSeatQuantityStore(context.CreateContextFactory());
            var started = await store.PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                1,
                null,
                ChangeTime,
                CancellationToken.None);
            operationId = started.Operation!.Id;
        }

        var blocked = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "failed-decrease-blocked@example.com",
            ChangeTime.AddMinutes(1));
        await using (var failureContext = database.CreateContext())
        {
            Assert.That(
                await new PostgresBillingSeatQuantityStore(failureContext.CreateContextFactory())
                    .RejectProviderMutationAsync(
                    operationId,
                    ChangeTime.AddMinutes(2),
                    CancellationToken.None),
                Is.Not.Null);
        }

        var restored = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "failed-decrease-restored@example.com",
            ChangeTime.AddMinutes(3));
        Assert.Multiple(() =>
        {
            Assert.That(blocked.Status, Is.EqualTo(OrganizationInvitationCreationStatus.SeatCapacityReached));
            Assert.That(restored.Status, Is.EqualTo(OrganizationInvitationCreationStatus.Created));
        });
    }

    [Test]
    public async Task CompletedIncreaseGrantsCapacityOnlyAfterProjection()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-completed-increase",
            "seat-change-completed-increase@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Completed seat increase",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 1);
        Guid operationId;
        await using (var context = database.CreateContext())
        {
            var started = await new PostgresBillingSeatQuantityStore(context.CreateContextFactory()).PrepareAsync(
                owner.UserId,
                organization.BillingAccountId,
                2,
                null,
                ChangeTime,
                CancellationToken.None);
            operationId = started.Operation!.Id;
        }

        var blocked = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "completed-increase-blocked@example.com",
            ChangeTime.AddMinutes(1));
        await ProjectSubscriptionAsync(
            database,
            organization.BillingAccountId,
            2,
            CommercialSubscriptionStatus.Active,
            ChangeTime.AddMinutes(2));
        await using (var completionContext = database.CreateContext())
        {
            Assert.That(
                await new PostgresBillingSeatQuantityStore(completionContext.CreateContextFactory())
                    .ApplySubscriptionAndResolveAsync(
                    operationId,
                    AuthoritativeSubscription(
                        organization.BillingAccountId,
                        seatQuantity: 2,
                        CommercialSubscriptionStatus.Active,
                        ChangeTime.AddMinutes(3)),
                    ChangeTime.AddMinutes(3),
                    CancellationToken.None),
                Is.Not.Null);
        }

        var available = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "completed-increase-available@example.com",
            ChangeTime.AddMinutes(4));
        Assert.Multiple(() =>
        {
            Assert.That(blocked.Status, Is.EqualTo(OrganizationInvitationCreationStatus.SeatCapacityReached));
            Assert.That(available.Status, Is.EqualTo(OrganizationInvitationCreationStatus.Created));
        });
    }

    [Test]
    public async Task ProjectionAndOperationClosureRollBackTogetherBeforeAProviderRetry()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seat-change-lost-projection",
            "seat-change-lost-projection@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Lost seat projection",
            SignupTime.AddDays(1));
        await ProjectSubscriptionAsync(database, organization.BillingAccountId, 2);
        var provider = new StatefulSeatQuantityProvider(
            organization.BillingAccountId,
            currentQuantity: 2);
        Guid operationId;
        await using (var preparationContext = database.CreateContext())
        {
            var prepared = await new PostgresBillingSeatQuantityStore(preparationContext.CreateContextFactory())
                .PrepareAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    retryOperationId: null,
                    ChangeTime,
                    CancellationToken.None);
            operationId = prepared.Operation!.Id;
        }

        {
            await using var billingTest = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime, configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); }, interceptors: [
            new ThrowAfterSaveInterceptor()]);
            var service = billingTest.Service;
            Assert.ThrowsAsync<SimulatedPostSaveException>(
                async () => await service.ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    operationId,
                    CancellationToken.None));
        }

        await using (var interrupted = database.CreateContext())
        {
            var operation = interrupted.BillingOperations.Single();
            Assert.Multiple(() =>
            {
                Assert.That(operation.Status, Is.EqualTo(BillingOperationStatus.Pending));
                Assert.That(interrupted.CommercialSubscriptions.Single().SeatQuantity, Is.EqualTo(2));
            });
        }

        BillingSeatQuantityResult recovered;
        await using (var retryContext = database.CreateContext())
        {
            await using var billingTest2 = ServiceTestBase<BillingSeatQuantityService>.ForDatabase(
                database, ChangeTime.AddMinutes(3), configureServices: services => { services.RemoveAll<IBillingSeatQuantityProvider>(); services.AddSingleton<IBillingSeatQuantityProvider>(provider); });
            recovered = await billingTest2.Service
                .ChangeAsync(
                    owner.UserId,
                    organization.BillingAccountId,
                    5,
                    operationId,
                    CancellationToken.None);
        }

        await using var verification = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(recovered.Status, Is.EqualTo(BillingSeatQuantityStatus.Changed));
            Assert.That(provider.CallCount, Is.EqualTo(2));
            Assert.That(provider.MutationCount, Is.EqualTo(1));
            Assert.That(verification.CommercialSubscriptions.Single().SeatQuantity, Is.EqualTo(5));
            Assert.That(
                verification.BillingOperations.Single().Status,
                Is.EqualTo(BillingOperationStatus.Completed));
        });
    }

    private static async Task ProjectSubscriptionAsync(
        PostgresTestDatabase database,
        Guid billingAccountId,
        int seatQuantity,
        CommercialSubscriptionStatus status = CommercialSubscriptionStatus.Active,
        DateTimeOffset? projectedAt = null)
    {
        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, ChangeTime);
        await projectionTest.Service
            .ApplyAsync(
                billingAccountId,
                new CommercialSubscriptionProjection(
                    $"cus_{billingAccountId:N}",
                    $"sub_{billingAccountId:N}",
                    "price_test_monthly",
                    status,
                    seatQuantity,
                    cancelAtPeriodEnd: false,
                    SignupTime,
                    SignupTime.AddYears(1),
                    projectedAt ?? ChangeTime.AddHours(-1)),
                CancellationToken.None);
    }

    private static AuthoritativeCommercialSubscription AuthoritativeSubscription(
        Guid billingAccountId,
        int seatQuantity,
        CommercialSubscriptionStatus status,
        DateTimeOffset projectedAt,
        long providerReadRevision = 0)
    {
        return new(
            billingAccountId,
            new CommercialSubscriptionProjection(
                $"cus_{billingAccountId:N}",
                $"sub_{billingAccountId:N}",
                "price_test_monthly",
                status,
                seatQuantity,
                cancelAtPeriodEnd: false,
                SignupTime,
                SignupTime.AddYears(1),
                projectedAt),
            providerReadRevision);
    }

    private static async Task<OrganizationInvitationCreationResult> CreateInvitationAsync(
        PostgresTestDatabase database,
        Guid actorUserId,
        Guid organizationId,
        string email,
        DateTimeOffset observedAt)
    {
        await using var invitationTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database, observedAt);
        return await invitationTest.Service
            .CreateAsync(
                actorUserId,
                organizationId,
                email,
                OrganizationRole.Member);
    }

    private sealed class RejectingSeatQuantityProvider : IBillingSeatQuantityProvider
    {
        public Task<BillingSeatQuantityProviderResult> ObserveAsync(
            BillingSeatQuantityProviderRequest request,
            CancellationToken cancellationToken)
        {
            throw new AssertionException("A terminal operation must not call the provider.");
        }

        public Task<BillingSeatQuantityProviderResult> ApplyAsync(
            BillingSeatQuantityProviderRequest request,
            DateTimeOffset automaticReplayEndsAt,
            CancellationToken cancellationToken)
        {
            throw new AssertionException("A terminal operation must not call the provider.");
        }
    }

    private sealed class ExpiringReplaySeatQuantityProvider(
        Guid billingAccountId,
        int previousQuantity,
        int targetQuantity)
        : IBillingSeatQuantityProvider
    {
        public DateTimeOffset ProviderObservedAt { get; set; }

        public int ObserveCount { get; private set; }

        public int ApplyCount { get; private set; }

        public Task<BillingSeatQuantityProviderResult> ObserveAsync(
            BillingSeatQuantityProviderRequest request,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Assert.Multiple(() =>
            {
                Assert.That(request.BillingAccountId, Is.EqualTo(billingAccountId));
                Assert.That(request.PreviousSeatQuantity, Is.EqualTo(previousQuantity));
                Assert.That(request.SeatQuantity, Is.EqualTo(targetQuantity));
            });
            ObserveCount++;
            return Task.FromResult(
                new BillingSeatQuantityProviderResult(
                    BillingSeatQuantityProviderStatus.MutationRequired,
                    AuthoritativeSubscription(
                        billingAccountId,
                        previousQuantity,
                        CommercialSubscriptionStatus.Active,
                        ProviderObservedAt)));
        }

        public Task<BillingSeatQuantityProviderResult> ApplyAsync(
            BillingSeatQuantityProviderRequest request,
            DateTimeOffset automaticReplayEndsAt,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            ApplyCount++;
            throw new BillingSeatQuantityProviderUnavailableException(
                "Simulated indeterminate provider mutation.");
        }
    }

    private sealed class RecoveringSeatQuantityProvider(
        Guid billingAccountId,
        int currentQuantity)
        : IBillingSeatQuantityProvider
    {
        private int _currentQuantity = currentQuantity;
        private int _projectionSequence;

        public int ApplyCount { get; private set; }

        public int MutationCount { get; private set; }

        public Task<BillingSeatQuantityProviderResult> ObserveAsync(
            BillingSeatQuantityProviderRequest request,
            CancellationToken cancellationToken)
        {
            return Task.FromResult(
                Result(
                    request,
                    _currentQuantity == request.SeatQuantity
                        ? BillingSeatQuantityProviderStatus.Applied
                        : BillingSeatQuantityProviderStatus.MutationRequired,
                    _currentQuantity));
        }

        public Task<BillingSeatQuantityProviderResult> ApplyAsync(
            BillingSeatQuantityProviderRequest request,
            DateTimeOffset automaticReplayEndsAt,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            ApplyCount++;
            if (ApplyCount == 1)
            {
                throw new BillingSeatQuantityProviderUnavailableException(
                    "Simulated interruption before the provider mutation.");
            }

            Assert.That(_currentQuantity, Is.EqualTo(request.PreviousSeatQuantity));
            _currentQuantity = request.SeatQuantity;
            MutationCount++;
            return Task.FromResult(
                Result(
                    request,
                    BillingSeatQuantityProviderStatus.Applied,
                    _currentQuantity));
        }

        private BillingSeatQuantityProviderResult Result(
            BillingSeatQuantityProviderRequest request,
            BillingSeatQuantityProviderStatus status,
            int quantity)
        {
            Assert.That(request.BillingAccountId, Is.EqualTo(billingAccountId));
            var providerObservedAt = ChangeTime.AddMinutes(
                Interlocked.Increment(ref _projectionSequence));
            return new BillingSeatQuantityProviderResult(
                status,
                new AuthoritativeCommercialSubscription(
                    billingAccountId,
                    new CommercialSubscriptionProjection(
                        $"cus_{billingAccountId:N}",
                        $"sub_{billingAccountId:N}",
                        "price_test_monthly",
                        CommercialSubscriptionStatus.Active,
                        quantity,
                        cancelAtPeriodEnd: false,
                        SignupTime,
                        SignupTime.AddYears(1),
                        providerObservedAt)));
        }
    }

    private sealed class ConcurrentTargetSeatQuantityProvider(
        Guid billingAccountId,
        int targetQuantity)
        : IBillingSeatQuantityProvider
    {
        private readonly DatabaseCommandBarrier _barrier = new(participantCount: 2);

        public int ObservationCount => _barrier.ArrivedCount;

        public async Task<BillingSeatQuantityProviderResult> ObserveAsync(
            BillingSeatQuantityProviderRequest request,
            CancellationToken cancellationToken)
        {
            Assert.Multiple(() =>
            {
                Assert.That(request.BillingAccountId, Is.EqualTo(billingAccountId));
                Assert.That(request.SeatQuantity, Is.EqualTo(targetQuantity));
            });
            await _barrier.SignalAndWaitAsync(cancellationToken);
            return new BillingSeatQuantityProviderResult(
                BillingSeatQuantityProviderStatus.Applied,
                new AuthoritativeCommercialSubscription(
                    billingAccountId,
                    new CommercialSubscriptionProjection(
                        $"cus_{billingAccountId:N}",
                        $"sub_{billingAccountId:N}",
                        "price_test_monthly",
                        CommercialSubscriptionStatus.Active,
                        targetQuantity,
                        cancelAtPeriodEnd: false,
                        SignupTime,
                        SignupTime.AddYears(1),
                        ChangeTime.AddMinutes(1))));
        }

        public Task<BillingSeatQuantityProviderResult> ApplyAsync(
            BillingSeatQuantityProviderRequest request,
            DateTimeOffset automaticReplayEndsAt,
            CancellationToken cancellationToken)
        {
            throw new AssertionException(
                "An already-applied provider quantity must not be mutated.");
        }
    }

    private sealed class ConcurrentIdempotentSeatQuantityProvider(
        Guid billingAccountId,
        int previousQuantity,
        int targetQuantity)
        : IBillingSeatQuantityProvider
    {
        private readonly DatabaseCommandBarrier _observationBarrier = new(participantCount: 2);
        private readonly object _mutationLock = new();
        private Guid? _operationId;
        private bool _mutationApplied;
        private int _applyCount;
        private int _mutationCount;

        public int ObservationCount => _observationBarrier.ArrivedCount;

        public int ApplyCount => Volatile.Read(ref _applyCount);

        public int MutationCount => Volatile.Read(ref _mutationCount);

        public async Task<BillingSeatQuantityProviderResult> ObserveAsync(
            BillingSeatQuantityProviderRequest request,
            CancellationToken cancellationToken)
        {
            Validate(request);
            var observation = Result(
                BillingSeatQuantityProviderStatus.MutationRequired,
                previousQuantity,
                projectedOffsetMinutes: 1);
            await _observationBarrier.SignalAndWaitAsync(cancellationToken);
            return observation;
        }

        public Task<BillingSeatQuantityProviderResult> ApplyAsync(
            BillingSeatQuantityProviderRequest request,
            DateTimeOffset automaticReplayEndsAt,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Validate(request);
            Interlocked.Increment(ref _applyCount);
            lock (_mutationLock)
            {
                if (!_mutationApplied)
                {
                    _mutationApplied = true;
                    Interlocked.Increment(ref _mutationCount);
                }

                Assert.That(_mutationApplied, Is.True);
            }

            return Task.FromResult(
                Result(
                    BillingSeatQuantityProviderStatus.Applied,
                    targetQuantity,
                    projectedOffsetMinutes: 2));
        }

        private void Validate(BillingSeatQuantityProviderRequest request)
        {
            Assert.Multiple(() =>
            {
                Assert.That(request.BillingAccountId, Is.EqualTo(billingAccountId));
                Assert.That(request.PreviousSeatQuantity, Is.EqualTo(previousQuantity));
                Assert.That(request.SeatQuantity, Is.EqualTo(targetQuantity));
            });
            _operationId ??= request.BillingOperationId;
            Assert.That(request.BillingOperationId, Is.EqualTo(_operationId));
        }

        private BillingSeatQuantityProviderResult Result(
            BillingSeatQuantityProviderStatus status,
            int quantity,
            int projectedOffsetMinutes)
        {
            return new(
                status,
                AuthoritativeSubscription(
                    billingAccountId,
                    quantity,
                    CommercialSubscriptionStatus.Active,
                    ChangeTime.AddMinutes(projectedOffsetMinutes)));
        }
    }

    private sealed class StatefulSeatQuantityProvider(
        Guid billingAccountId,
        int currentQuantity)
        : IBillingSeatQuantityProvider
    {
        private int _currentQuantity = currentQuantity;
        private int _projectionSequence;

        public int CallCount { get; private set; }

        public int MutationCount { get; private set; }

        public Task<BillingSeatQuantityProviderResult> ObserveAsync(
            BillingSeatQuantityProviderRequest request,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            CallCount++;
            Assert.That(request.BillingAccountId, Is.EqualTo(billingAccountId));
            var status = _currentQuantity == request.SeatQuantity
                ? BillingSeatQuantityProviderStatus.Applied
                : _currentQuantity == request.PreviousSeatQuantity
                    ? BillingSeatQuantityProviderStatus.MutationRequired
                    : BillingSeatQuantityProviderStatus.Superseded;
            return Task.FromResult(Result(status));
        }

        public Task<BillingSeatQuantityProviderResult> ApplyAsync(
            BillingSeatQuantityProviderRequest request,
            DateTimeOffset automaticReplayEndsAt,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Assert.That(_currentQuantity, Is.EqualTo(request.PreviousSeatQuantity));
            _currentQuantity = request.SeatQuantity;
            MutationCount++;
            return Task.FromResult(Result(BillingSeatQuantityProviderStatus.Applied));
        }

        private BillingSeatQuantityProviderResult Result(
            BillingSeatQuantityProviderStatus status)
        {
            var providerObservedAt = ChangeTime.AddMinutes(
                Interlocked.Increment(ref _projectionSequence));
            return new BillingSeatQuantityProviderResult(
                status,
                new AuthoritativeCommercialSubscription(
                    billingAccountId,
                    new CommercialSubscriptionProjection(
                        $"cus_{billingAccountId:N}",
                        $"sub_{billingAccountId:N}",
                        "price_test_monthly",
                        CommercialSubscriptionStatus.Active,
                        _currentQuantity,
                        cancelAtPeriodEnd: false,
                        SignupTime,
                        SignupTime.AddYears(1),
                        providerObservedAt)));
        }
    }
}
