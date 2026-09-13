using Styrhous.Licensing.Infrastructure.Organizations;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using static Styrhous.Licensing.Tests.Persistence.ApplicationIdentifierTestAssertions;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationApplicationIdentifierPersistenceTests
{
    private static readonly DateTimeOffset ObservedAt =
        new(2026, 8, 31, 11, 0, 0, TimeSpan.Zero);

    [Test]
    public async Task OrganizationCreationResolvesOwnersRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var serviceTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database,
            ObservedAt,
            configureServices: null,
            configureHostServices: null,
            counter);
        var service = serviceTest.Service;

        await AssertSingleUsesDatabaseAsync(
            ownerUserId => service.CreateAsync(ownerUserId, "Example Organization"),
            counter);
    }

    [Test]
    public async Task InvitationCreationResolvesRelationshipsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCreationService>.ForDatabase(
            database,
            ObservedAt,
            configureServices: null,
            configureHostServices: null,
            counter);
        var service = serviceTest.Service;

        await AssertPairUsesDatabaseAsync(
            (actorUserId, organizationId) => service.CreateAsync(
                actorUserId,
                organizationId,
                "member@example.com",
                OrganizationRole.Member),
            counter);
    }

    [Test]
    public async Task InvitationAcceptanceResolvesActorsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var serviceTest = ServiceTestBase<OrganizationInvitationAcceptanceService>.ForDatabase(
            database,
            ObservedAt,
            configureServices: null,
            configureHostServices: null,
            counter);
        var service = serviceTest.Service;

        await AssertSingleUsesDatabaseAsync(
            actorUserId => service.AcceptAsync(actorUserId, "valid-secret"),
            counter);
    }

    [Test]
    public async Task InvitationCancellationResolvesRelationshipsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var serviceTest = ServiceTestBase<OrganizationInvitationCancellationService>.ForDatabase(
            database,
            ObservedAt,
            configureServices: null,
            configureHostServices: null,
            counter);
        var service = serviceTest.Service;

        await AssertTripleUsesDatabaseAsync(
            (actorUserId, organizationId, invitationId) => service.CancelAsync(
                actorUserId,
                organizationId,
                invitationId),
            counter);
    }

    [Test]
    public async Task InvitationListingResolvesRelationshipsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var serviceTest = ServiceTestBase<OrganizationInvitationListingService>.ForDatabase(
            database,
            ObservedAt,
            configureServices: null,
            configureHostServices: null,
            counter);
        var service = serviceTest.Service;

        await AssertPairUsesDatabaseAsync(
            (actorUserId, organizationId) => service.ListAsync(actorUserId, organizationId),
            counter);
    }

    [Test]
    public async Task InvitationResendResolvesRelationshipsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var serviceTest = ServiceTestBase<OrganizationInvitationResendService>.ForDatabase(
            database,
            ObservedAt,
            configureServices: null,
            configureHostServices: null,
            counter);
        var service = serviceTest.Service;

        await AssertTripleUsesDatabaseAsync(
            (actorUserId, organizationId, invitationId) => service.ResendAsync(
                actorUserId,
                organizationId,
                invitationId),
            counter);
    }

    [Test]
    public async Task OrganizationListingResolvesUsersRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var serviceTest = ServiceTestBase<OrganizationListingService>.ForDatabase(
            database,
            ObservedAt,
            configureServices: null,
            configureHostServices: null,
            counter);
        var service = serviceTest.Service;

        await AssertSingleUsesDatabaseAsync(
            userId => service.ListForUserAsync(userId),
            counter);
    }

    [Test]
    public async Task MemberListingResolvesRelationshipsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var serviceTest = ServiceTestBase<OrganizationMemberListingService>.ForDatabase(
            database,
            ObservedAt,
            configureServices: null,
            configureHostServices: null,
            counter);
        var service = serviceTest.Service;

        await AssertPairUsesDatabaseAsync(
            (actorUserId, organizationId) => service.ListAsync(actorUserId, organizationId),
            counter);
    }

    [Test]
    public async Task MemberRemovalResolvesRelationshipsRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var counter = new DatabaseCommandCounterInterceptor();
        await using var serviceTest = ServiceTestBase<OrganizationMemberRemovalService>.ForDatabase(
            database,
            ObservedAt,
            configureServices: null,
            configureHostServices: null,
            counter);
        var service = serviceTest.Service;

        await AssertTripleUsesDatabaseAsync(
            (actorUserId, organizationId, membershipId) => service.RemoveAsync(
                actorUserId,
                organizationId,
                membershipId),
            counter);
    }

}
