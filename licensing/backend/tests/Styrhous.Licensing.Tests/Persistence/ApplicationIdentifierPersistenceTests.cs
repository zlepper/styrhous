using Styrhous.Licensing.Application.Billing;
using Styrhous.Licensing.Application.Devices;
using Styrhous.Licensing.Application.Entitlements;
using Styrhous.Licensing.Domain.Devices;
using Styrhous.Licensing.Domain.Billing;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.ApplicationIdentifierTestAssertions;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class ApplicationIdentifierPersistenceTests
{
    private static readonly DateTimeOffset ObservedAt =
        new(2026, 8, 31, 10, 0, 0, TimeSpan.Zero);

    [Test]
    public async Task DeviceActivationResolvesRelationshipsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();


        await AssertPairUsesDatabaseAsync(
            (userId, seatId) => DevicePersistenceScenario.ActivateAsync(database.CreateContextFactory(counter), ObservedAt,
                userId,
                seatId,
                CreateInstallation()),
            counter);
    }

    [Test]
    public async Task EntitlementCheckResolvesRelationshipsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var checkTest = ServiceTestBase<DeviceEntitlementCheckService>.ForDatabase(
            database, ObservedAt, interceptors: [counter]);
        var service = checkTest.Service;

        await AssertPairUsesDatabaseAsync(
            (userId, activationId) => service.CheckAsync(userId, activationId),
            counter);
    }

    [Test]
    public async Task DeviceListingResolvesRelationshipsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var listingTest = ServiceTestBase<DeviceListingService>.ForDatabase(
            database, ObservedAt, interceptors: [counter]);
        var service = listingTest.Service;

        await AssertPairUsesDatabaseAsync(
            (userId, seatId) => service.ListActiveAsync(userId, seatId),
            counter);
    }

    [Test]
    public async Task DeviceRevocationResolvesRelationshipsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var revocationTest = ServiceTestBase<DeviceRevocationService>.ForDatabase(
            database, ObservedAt, interceptors: [counter]);
        var service = revocationTest.Service;

        await AssertPairUsesDatabaseAsync(
            (userId, activationId) => service.RevokeAsync(userId, activationId),
            counter);
    }

    [Test]
    public async Task EntitlementResolutionResolvesUsersRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var entitlementTest = ServiceTestBase<EntitlementResolutionService>.ForDatabase(
            database, ObservedAt, interceptors: [counter]);
        var service = entitlementTest.Service;

        await AssertSingleUsesDatabaseAsync(
            userId => service.ListForUserAsync(userId),
            counter);
    }

    [Test]
    public async Task CommercialProjectionResolvesBillingAccountsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var projectionTest = ServiceTestBase<CommercialSubscriptionProjectionService>.ForDatabase(
            database, ObservedAt, interceptors: [counter]);
        var service = projectionTest.Service;
        var projection = new CommercialSubscriptionProjection(
            "cus_identifier",
            "sub_identifier",
            "price_identifier",
            CommercialSubscriptionStatus.Active,
            1,
            false,
            ObservedAt,
            ObservedAt.AddMonths(1),
            ObservedAt);

        await AssertSingleUsesDatabaseAsync(
            billingAccountId => service.ApplyAsync(billingAccountId, projection),
            counter);
    }

    [Test]
    public async Task BillingAccountListingResolvesUsersRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var listingTest = ServiceTestBase<BillingAccountListingService>.ForDatabase(
            database, ObservedAt, interceptors: [counter]);
        var service = listingTest.Service;

        await AssertSingleUsesDatabaseAsync(
            userId => service.ListForUserAsync(userId),
            counter);
    }

    private static DesktopInstallation CreateInstallation()
    {
        return DesktopInstallation.Create(
            Guid.CreateVersion7(),
            "Work laptop",
            "linux",
            "x86_64",
            "1.2.3");
    }
}
