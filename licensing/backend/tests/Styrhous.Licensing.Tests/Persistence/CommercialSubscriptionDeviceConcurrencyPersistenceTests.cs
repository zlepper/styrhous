using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.DevicePersistenceScenario;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class CommercialSubscriptionDeviceConcurrencyPersistenceTests
{
    [TestCase(CommercialSubscriptionStatus.Active)]
    [TestCase(CommercialSubscriptionStatus.PastDue)]
    public async Task PaidSubscriptionAllowsActivationAfterTrialExpiry(
        CommercialSubscriptionStatus status)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            $"paid-activation-{status}",
            "activation@example.com");
        await CreateSubscriptionAsync(
            database,
            signup.PersonalBillingAccountId,
            status);

        var result = await DevicePersistenceScenario.ActivateAsync(database.CreateContextFactory(), SignupTime.AddDays(31), signup.UserId, signup.SeatId, CreateInstallation(1));

        Assert.That(result.Status, Is.EqualTo(DeviceActivationStatus.Activated));
    }

    [Test]
    public async Task PaidSubscriptionDoesNotAllowActivationAtThePeriodEnd()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "paid-activation-period-end",
            "activation@example.com");
        await CreateSubscriptionAsync(
            database,
            signup.PersonalBillingAccountId,
            CommercialSubscriptionStatus.Active);

        var result = await DevicePersistenceScenario.ActivateAsync(database.CreateContextFactory(), SignupTime.AddDays(60), signup.UserId, signup.SeatId, CreateInstallation(1));

        Assert.That(result.Status, Is.EqualTo(DeviceActivationStatus.SeatNotEligible));
    }

    [Test]
    public async Task ActivationObservesAnInFlightSubscriptionDeactivation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "paid-activation-race",
            "activation@example.com");
        await CreateSubscriptionAsync(
            database,
            signup.PersonalBillingAccountId,
            CommercialSubscriptionStatus.Active);
        var projectionSaveGate = new DatabaseCommandGate();
        var entitlementBillingSerializationGate = new DatabaseCommandGate();

        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime, interceptors: [
            new SavedChangesGateInterceptor(projectionSaveGate)]);
        var projectionTask = projectionTest.Service.ApplyAsync(
            signup.PersonalBillingAccountId,
            CreateProjection(
                CommercialSubscriptionStatus.Unpaid,
                SignupTime.AddDays(31)));

        DeviceActivationResult activationResult;
        CommercialSubscriptionProjectionResult projectionResult;
        try
        {
            await projectionSaveGate.WaitUntilReachedAsync();
            var activationTask = DevicePersistenceScenario.ActivateAsync(database.CreateContextFactory(
                            new DatabaseCommandGateInterceptor(
                                entitlementBillingSerializationGate,
                                "UPDATE billing_accounts")), SignupTime.AddDays(31), signup.UserId, signup.SeatId, CreateInstallation(1));
            await entitlementBillingSerializationGate.WaitUntilReachedAsync();
            entitlementBillingSerializationGate.Release();
            projectionSaveGate.Release();

            projectionResult = await projectionTask;
            activationResult = await activationTask;
        }
        finally
        {
            entitlementBillingSerializationGate.Release();
            projectionSaveGate.Release();
        }

        await using var verificationContext = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                projectionResult.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.Updated));
            Assert.That(activationResult.Status, Is.EqualTo(DeviceActivationStatus.SeatNotEligible));
            Assert.That(verificationContext.DeviceActivations, Is.Empty);
        });
    }

    [Test]
    public async Task EntitlementCheckObservesAnInFlightSubscriptionDeactivation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "paid-check-race",
            "check@example.com");
        var activation = await ActivateAsync(database, signup, 1, SignupTime);
        await CreateSubscriptionAsync(
            database,
            signup.PersonalBillingAccountId,
            CommercialSubscriptionStatus.Active);
        var projectionSaveGate = new DatabaseCommandGate();
        var entitlementBillingSerializationGate = new DatabaseCommandGate();

        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime, interceptors: [
            new SavedChangesGateInterceptor(projectionSaveGate)]);
        var projectionTask = projectionTest.Service.ApplyAsync(
            signup.PersonalBillingAccountId,
            CreateProjection(
                CommercialSubscriptionStatus.Unpaid,
                SignupTime.AddDays(31)));

        DeviceEntitlementCheckResult checkResult;
        CommercialSubscriptionProjectionResult projectionResult;
        try
        {
            await projectionSaveGate.WaitUntilReachedAsync();
            await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
                database, SignupTime.AddDays(31), interceptors: [
            new DatabaseCommandGateInterceptor(
                entitlementBillingSerializationGate,
                "UPDATE billing_accounts")]);
            var checkTask = checkTest.Service
                .CheckAsync(signup.UserId, activation.ActivationId!.Value);
            await entitlementBillingSerializationGate.WaitUntilReachedAsync();
            entitlementBillingSerializationGate.Release();
            projectionSaveGate.Release();

            projectionResult = await projectionTask;
            checkResult = await checkTask;
        }
        finally
        {
            entitlementBillingSerializationGate.Release();
            projectionSaveGate.Release();
        }

        await using var verificationContext = database.CreateContext();
        var persistedActivation = await verificationContext.DeviceActivations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                projectionResult.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.Updated));
            Assert.That(checkResult.Status, Is.EqualTo(DeviceEntitlementCheckStatus.Ineligible));
            Assert.That(checkResult.ReasonCode, Is.EqualTo(EntitlementReasonCodes.SubscriptionInactive));
            Assert.That(persistedActivation.LastSeenAt, Is.EqualTo(SignupTime));
        });
    }

    [Test]
    public async Task EntitlementCheckObservesAnInFlightSeatQuantityReduction()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "paid-check-reduction-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "paid-check-reduction-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Paid Check Reduction",
            SignupTime.AddDays(1));
        var memberSeat = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            joinedAt: SignupTime.AddDays(2));
        await CreateSubscriptionAsync(
            database,
            organization.BillingAccountId,
            CommercialSubscriptionStatus.Active,
            seatQuantity: 2);
        var activation = await ActivateAsync(
            database,
            member.UserId,
            memberSeat.SeatId,
            installationNumber: 1,
            SignupTime.AddDays(31));
        Assert.That(activation.Status, Is.EqualTo(DeviceActivationStatus.Activated));
        var projectionSaveGate = new DatabaseCommandGate();
        var entitlementBillingSerializationGate = new DatabaseCommandGate();

        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime, interceptors: [
            new SavedChangesGateInterceptor(projectionSaveGate)]);
        var projectionTask = projectionTest.Service.ApplyAsync(
            organization.BillingAccountId,
            CreateProjection(
                CommercialSubscriptionStatus.Active,
                SignupTime.AddDays(32),
                seatQuantity: 1));

        DeviceEntitlementCheckResult checkResult;
        CommercialSubscriptionProjectionResult projectionResult;
        try
        {
            await projectionSaveGate.WaitUntilReachedAsync();
            await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
                database, SignupTime.AddDays(32), interceptors: [
            new DatabaseCommandGateInterceptor(
                entitlementBillingSerializationGate,
                "UPDATE billing_accounts")]);
            var checkTask = checkTest.Service
                .CheckAsync(member.UserId, activation.ActivationId!.Value);
            await entitlementBillingSerializationGate.WaitUntilReachedAsync();
            entitlementBillingSerializationGate.Release();
            projectionSaveGate.Release();

            projectionResult = await projectionTask;
            checkResult = await checkTask;
        }
        finally
        {
            entitlementBillingSerializationGate.Release();
            projectionSaveGate.Release();
        }

        await using var verificationContext = database.CreateContext();
        var persistedActivation = await verificationContext.DeviceActivations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(
                projectionResult.Status,
                Is.EqualTo(CommercialSubscriptionProjectionStatus.Updated));
            Assert.That(checkResult.Status, Is.EqualTo(DeviceEntitlementCheckStatus.Ineligible));
            Assert.That(
                checkResult.ReasonCode,
                Is.EqualTo(EntitlementReasonCodes.SubscriptionSeatCapacityExceeded));
            Assert.That(persistedActivation.LastSeenAt, Is.EqualTo(SignupTime.AddDays(31)));
        });
    }

    private static async Task CreateSubscriptionAsync(
        PostgresTestDatabase database,
        Guid billingAccountId,
        CommercialSubscriptionStatus status,
        int seatQuantity = 1)
    {
        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, SignupTime);
        var result = await projectionTest.Service.ApplyAsync(
            billingAccountId,
            CreateProjection(
                status,
                SignupTime.AddDays(30),
                seatQuantity));
        Assert.That(result.Status, Is.EqualTo(CommercialSubscriptionProjectionStatus.Created));
    }

    private static CommercialSubscriptionProjection CreateProjection(
        CommercialSubscriptionStatus status,
        DateTimeOffset projectedAt,
        int seatQuantity = 1)
    {
        return new(
            "cus_device_race",
            "sub_device_race",
            "price_device_race",
            status,
            seatQuantity,
            cancelAtPeriodEnd: false,
            SignupTime.AddDays(30),
            SignupTime.AddDays(60),
            projectedAt);
    }
}
