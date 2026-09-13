using System.Data.Common;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class DeviceEntitlementCheckPersistenceTests
{
    [Test]
    public async Task EligibleCheckRefreshesLastSeenOnlyAtTheOneHourBoundary()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);

        var early = await CheckAsync(
            database,
            signup.UserId,
            activation.ActivationId!.Value,
            SignupTime.AddHours(1).AddTicks(-1));
        var boundary = await CheckAsync(
            database,
            signup.UserId,
            activation.ActivationId.Value,
            SignupTime.AddHours(1));
        var throttled = await CheckAsync(
            database,
            signup.UserId,
            activation.ActivationId.Value,
            SignupTime.AddHours(1).AddMinutes(59));
        var secondBoundary = await CheckAsync(
            database,
            signup.UserId,
            activation.ActivationId.Value,
            SignupTime.AddHours(2));

        Assert.Multiple(() =>
        {
            Assert.That(early.Status, Is.EqualTo(DeviceEntitlementCheckStatus.Eligible));
            Assert.That(early.ReasonCode, Is.EqualTo(EntitlementReasonCodes.ActiveTrial));
            Assert.That(early.Entitlement!.SeatId, Is.EqualTo(signup.SeatId));
            Assert.That(early.Device!.LastSeenAt, Is.EqualTo(SignupTime));
            Assert.That(boundary.Status, Is.EqualTo(DeviceEntitlementCheckStatus.Eligible));
            Assert.That(boundary.Device!.LastSeenAt, Is.EqualTo(SignupTime.AddHours(1)));
            Assert.That(throttled.Device!.LastSeenAt, Is.EqualTo(SignupTime.AddHours(1)));
            Assert.That(secondBoundary.Device!.LastSeenAt, Is.EqualTo(SignupTime.AddHours(2)));
        });
        await using var context = database.CreateContext();
        Assert.That(
            (await context.DeviceActivations.SingleAsync()).LastSeenAt,
            Is.EqualTo(SignupTime.AddHours(2)));
    }

    [Test]
    public async Task ConcurrentChecksWriteLastSeenAtOnlyOncePerHour()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        var counter = new DeviceLastSeenUpdateCountingInterceptor();
        await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, SignupTime.AddHours(1), interceptors: [counter]);
        var first = checkTest.Service;
        await using var checkTest2 = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, SignupTime.AddHours(1), interceptors: [counter]);
        var second = checkTest2.Service;

        var results = await Task.WhenAll(
            first.CheckAsync(signup.UserId, activation.ActivationId!.Value),
            second.CheckAsync(signup.UserId, activation.ActivationId.Value));

        Assert.Multiple(() =>
        {
            Assert.That(
                results.Select(result => result.Status),
                Is.All.EqualTo(DeviceEntitlementCheckStatus.Eligible));
            Assert.That(counter.UpdateCount, Is.EqualTo(1));
        });
    }

    [Test]
    public async Task IneligibleCheckReturnsEvaluationWithoutRefreshingLastSeen()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);

        var result = await CheckAsync(
            database,
            signup.UserId,
            activation.ActivationId!.Value,
            SignupTime.AddDays(30));

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(DeviceEntitlementCheckStatus.Ineligible));
            Assert.That(result.ReasonCode, Is.EqualTo(EntitlementReasonCodes.TrialExpired));
            Assert.That(result.Entitlement!.State, Is.EqualTo(EntitlementState.Evaluation));
            Assert.That(result.Device!.LastSeenAt, Is.EqualTo(SignupTime));
        });
        await using var context = database.CreateContext();
        Assert.That(
            (await context.DeviceActivations.SingleAsync()).LastSeenAt,
            Is.EqualTo(SignupTime));
    }

    [Test]
    public async Task PaidSubscriptionKeepsDeviceEligibleAfterTrialExpiry()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        await using (var context = database.CreateContext())
        {
            context.CommercialSubscriptions.Add(
                CommercialSubscription.Create(
                    signup.PersonalBillingAccountId,
                    new CommercialSubscriptionProjection(
                        "cus_device_check",
                        "sub_device_check",
                        "price_device_check",
                        CommercialSubscriptionStatus.Active,
                        seatQuantity: 1,
                        cancelAtPeriodEnd: false,
                        SignupTime.AddDays(30),
                        SignupTime.AddDays(60),
                        SignupTime.AddDays(30))));
            await context.SaveChangesAsync();
        }

        var result = await CheckAsync(
            database,
            signup.UserId,
            activation.ActivationId!.Value,
            SignupTime.AddDays(31));

        Assert.Multiple(() =>
        {
            Assert.That(result.Status, Is.EqualTo(DeviceEntitlementCheckStatus.Eligible));
            Assert.That(result.ReasonCode, Is.EqualTo(EntitlementReasonCodes.ActiveSubscription));
            Assert.That(result.Entitlement!.State, Is.EqualTo(EntitlementState.Commercial));
            Assert.That(result.Device!.LastSeenAt, Is.EqualTo(SignupTime.AddDays(31)));
        });
    }

    [Test]
    public async Task TrialTransferChangesWhichSeatPassesEntitlementChecks()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var personalActivation = await ActivateAsync(database, signup, 1, SignupTime);
        var organization = await CreateOrganizationAsync(
            database,
            signup.UserId,
            SignupTime.AddDays(1));

        var personalCheck = await CheckAsync(
            database,
            signup.UserId,
            personalActivation.ActivationId!.Value,
            SignupTime.AddDays(2));
        var organizationActivation = await ActivateAsync(
            database,
            signup.UserId,
            organization.SeatId,
            2,
            SignupTime.AddDays(2));
        var organizationCheck = await CheckAsync(
            database,
            signup.UserId,
            organizationActivation.ActivationId!.Value,
            SignupTime.AddDays(2).AddHours(1));

        Assert.Multiple(() =>
        {
            Assert.That(personalCheck.Status, Is.EqualTo(DeviceEntitlementCheckStatus.Ineligible));
            Assert.That(personalCheck.ReasonCode, Is.EqualTo(EntitlementReasonCodes.NoValidEntitlement));
            Assert.That(personalCheck.Device!.LastSeenAt, Is.EqualTo(SignupTime));
            Assert.That(organizationCheck.Status, Is.EqualTo(DeviceEntitlementCheckStatus.Eligible));
            Assert.That(organizationCheck.ReasonCode, Is.EqualTo(EntitlementReasonCodes.ActiveTrial));
            Assert.That(organizationCheck.Entitlement!.ValidFrom, Is.EqualTo(SignupTime));
            Assert.That(
                organizationCheck.Entitlement.ValidUntil,
                Is.EqualTo(SignupTime.AddDays(30)));
        });
    }

    [Test]
    public async Task EntitlementCheckAndTrialTransferUseTheSameUserLockOrder()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        var barrier = new DatabaseCommandBarrier(participantCount: 2);

        await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var checkTask = checkTest.Service
            .CheckAsync(signup.UserId, activation.ActivationId!.Value);
        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var organizationTask = organizationTest.Service
            .CreateAsync(signup.UserId, "Concurrent Check Organization");
        await Task.WhenAll(checkTask, organizationTask);
        var check = await checkTask;
        var organization = await organizationTask;
        var expectedReasonCode = check.Status == DeviceEntitlementCheckStatus.Eligible
            ? EntitlementReasonCodes.ActiveTrial
            : EntitlementReasonCodes.NoValidEntitlement;

        Assert.Multiple(() =>
        {
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
            Assert.That(organization.TrialWasTransferred, Is.True);
            Assert.That(
                check.Status,
                Is.AnyOf(
                    DeviceEntitlementCheckStatus.Eligible,
                    DeviceEntitlementCheckStatus.Ineligible));
            Assert.That(
                check.ReasonCode,
                Is.EqualTo(expectedReasonCode));
        });
        await using var context = database.CreateContext();
        var trial = await context.Trials.SingleAsync();
        var persistedActivation = await context.DeviceActivations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(trial.BillingAccountId, Is.EqualTo(organization.BillingAccountId));
            Assert.That(
                persistedActivation.LastSeenAt,
                Is.EqualTo(
                    check.Status == DeviceEntitlementCheckStatus.Eligible
                        ? SignupTime.AddDays(1)
                        : SignupTime));
        });
    }

    [Test]
    public async Task EntitlementCheckAndStaleReplacementProduceOneAtomicOutcome()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var stale = await ActivateAsync(database, signup, 1, SignupTime);
        await ActivateAsync(database, signup, 2, SignupTime.AddDays(1));
        await ActivateAsync(database, signup, 3, SignupTime.AddDays(1));
        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        var activationFactory = database.CreateContextFactory(
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts"));

        await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, SignupTime.AddDays(8), interceptors: [
            new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var checkTask = checkTest.Service
            .CheckAsync(signup.UserId, stale.ActivationId!.Value);
        var replacementTask = DevicePersistenceScenario.ActivateAsync(activationFactory, SignupTime.AddDays(8),
                signup.UserId,
                signup.SeatId,
                CreateInstallation(4));
        await Task.WhenAll(checkTask, replacementTask);
        var check = await checkTask;
        var replacement = await replacementTask;

        Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
        await using var context = database.CreateContext();
        var persistedStale = await context.DeviceActivations.SingleAsync(
            candidate => candidate.Id == stale.ActivationId);
        var activeCount = await context.DeviceActivations.CountAsync(
            candidate => candidate.RevokedAt == null);
        var revokedCount = await context.DeviceActivations.CountAsync(
            candidate => candidate.RevokedAt != null);
        var auditCount = await context.AuditRecords.CountAsync();
        Assert.That(activeCount, Is.EqualTo(3));
        if (check.Status == DeviceEntitlementCheckStatus.Eligible)
        {
            Assert.Multiple(() =>
            {
                Assert.That(replacement.Status, Is.EqualTo(DeviceActivationStatus.DeviceLimitReached));
                Assert.That(persistedStale.RevokedAt, Is.Null);
                Assert.That(persistedStale.LastSeenAt, Is.EqualTo(SignupTime.AddDays(8)));
                Assert.That(revokedCount, Is.Zero);
                Assert.That(auditCount, Is.EqualTo(3));
            });
        }
        else
        {
            Assert.Multiple(() =>
            {
                Assert.That(check.Status, Is.EqualTo(DeviceEntitlementCheckStatus.DeviceNotActive));
                Assert.That(replacement.Status, Is.EqualTo(DeviceActivationStatus.Activated));
                Assert.That(replacement.RevokedActivationId, Is.EqualTo(stale.ActivationId));
                Assert.That(persistedStale.RevokedAt, Is.EqualTo(SignupTime.AddDays(8)));
                Assert.That(revokedCount, Is.EqualTo(1));
                Assert.That(auditCount, Is.EqualTo(5));
            });
        }
    }

    [Test]
    public async Task RevokedUnknownAndOtherUsersDevicesShareTheInactiveResult()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "check-owner", "owner@example.com");
        var other = await SignUpAsync(database, "check-other", "other@example.com");
        var first = await ActivateAsync(database, owner, 1, SignupTime);
        var stillActive = await ActivateAsync(database, owner, 2, SignupTime.AddDays(1));
        await ActivateAsync(database, owner, 3, SignupTime.AddDays(1));
        await ActivateAsync(database, owner, 4, SignupTime.AddDays(8));

        var revoked = await CheckAsync(
            database,
            owner.UserId,
            first.ActivationId!.Value,
            SignupTime.AddDays(8));
        var unknown = await CheckAsync(
            database,
            owner.UserId,
            Guid.CreateVersion7(),
            SignupTime.AddDays(8));
        var otherUser = await CheckAsync(
            database,
            other.UserId,
            stillActive.ActivationId!.Value,
            SignupTime.AddDays(8));

        foreach (var result in new[] { revoked, unknown, otherUser })
        {
            Assert.Multiple(() =>
            {
                Assert.That(result.Status, Is.EqualTo(DeviceEntitlementCheckStatus.DeviceNotActive));
                Assert.That(
                    result.ReasonCode,
                    Is.EqualTo(DeviceOperationReasonCodes.DeviceNotActive));
                Assert.That(result.Entitlement, Is.Null);
                Assert.That(result.Device, Is.Null);
            });
        }

        await using var context = database.CreateContext();
        var persistedActive = await context.DeviceActivations.SingleAsync(
            candidate => candidate.Id == stillActive.ActivationId);
        Assert.That(persistedActive.LastSeenAt, Is.EqualTo(SignupTime.AddDays(1)));
    }

    [Test]
    public async Task FailedLastSeenWriteRollsBack()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(database);
        var activation = await ActivateAsync(database, signup, 1, SignupTime);

        await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, SignupTime.AddHours(1), interceptors: [new ThrowAfterSaveInterceptor()]);
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await checkTest.Service
                .CheckAsync(signup.UserId, activation.ActivationId!.Value));

        await using var verificationContext = database.CreateContext();
        var persisted = await verificationContext.DeviceActivations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(persisted.DisplayName, Is.EqualTo("Device 1"));
            Assert.That(persisted.LastSeenAt, Is.EqualTo(SignupTime));
        });
    }

    [TestCase("user")]
    [TestCase("activation")]
    public async Task MissingOwnedIdentifiersDoNotRevealOrChangeDevices(string emptyIdentifier)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await LicensingPersistenceScenario.SignUpAsync(database);
        var activation = await DevicePersistenceScenario.ActivateAsync(database, owner, 1, SignupTime);
        await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, SignupTime);
        var service = checkTest.Service;

        var result = await service.CheckAsync(
            emptyIdentifier == "user" ? Guid.Empty : owner.UserId,
            emptyIdentifier == "activation" ? Guid.Empty : activation.ActivationId!.Value);
        Assert.That(result.Status, Is.EqualTo(DeviceEntitlementCheckStatus.DeviceNotActive));
        await using var verification = database.CreateContext();
        Assert.That((await verification.DeviceActivations.SingleAsync()).RevokedAt, Is.Null);
    }

    private static async Task<OrganizationCreationResult> CreateOrganizationAsync(
        PostgresTestDatabase database,
        Guid userId,
        DateTimeOffset observedAt)
    {
        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, observedAt);
        return await organizationTest.Service
            .CreateAsync(userId, "Check Organization");
    }

    private static async Task<DeviceEntitlementCheckResult> CheckAsync(
        PostgresTestDatabase database,
        Guid userId,
        Guid activationId,
        DateTimeOffset observedAt)
    {
        await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, observedAt);
        return await checkTest.Service
            .CheckAsync(userId, activationId);
    }

    private sealed class DeviceLastSeenUpdateCountingInterceptor : DbCommandInterceptor
    {
        private int _updateCount;

        public int UpdateCount => Volatile.Read(ref _updateCount);

        public override ValueTask<InterceptionResult<int>> NonQueryExecutingAsync(
            DbCommand command,
            CommandEventData eventData,
            InterceptionResult<int> result,
            CancellationToken cancellationToken = default)
        {
            CountLastSeenUpdate(command);
            return ValueTask.FromResult(result);
        }

        public override ValueTask<InterceptionResult<DbDataReader>> ReaderExecutingAsync(
            DbCommand command,
            CommandEventData eventData,
            InterceptionResult<DbDataReader> result,
            CancellationToken cancellationToken = default)
        {
            CountLastSeenUpdate(command);
            return ValueTask.FromResult(result);
        }

        private void CountLastSeenUpdate(DbCommand command)
        {
            if (command.CommandText.Contains(
                    "UPDATE device_activations",
                    StringComparison.OrdinalIgnoreCase))
            {
                Interlocked.Increment(ref _updateCount);
            }
        }
    }
}
