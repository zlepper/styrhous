using System.Net;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationListingApiTests
{
    private static readonly JsonSerializerOptions WebJson =
        new(JsonSerializerDefaults.Web);

    private static readonly string[] ListProperties =
    [
        "reasonCode",
        "organizations",
    ];

    private static readonly string[] OrganizationProperties =
    [
        "organizationId",
        "billingAccountId",
        "membershipId",
        "seatId",
        "name",
        "role",
        "productSeatAssigned",
        "deviceLimit",
        "joinedAt",
    ];

    [Test]
    public async Task AuthenticatedUserListsOrganizationsInMembershipOrder()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-organization-list",
            "api-organization-list@example.com");
        var first = await CreateOrganizationAsync(
            database,
            signup.UserId,
            "First Organization",
            SignupTime.AddDays(1));
        var second = await CreateOrganizationAsync(
            database,
            signup.UserId,
            "Second Organization",
            SignupTime.AddDays(2));
        await using (var context = database.CreateContext())
        {
            await context.Seats
                .Where(seat => seat.Id == second.SeatId)
                .ExecuteUpdateAsync(setters => setters.SetProperty(
                    seat => seat.ProductAccessEnabled,
                    false));
        }
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync("/api/organizations");
        var responseBody = await response.Content.ReadAsStringAsync();
        var body = JsonSerializer.Deserialize<OrganizationListResponse>(responseBody, WebJson);
        using var responseJson = JsonDocument.Parse(responseBody);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/json"));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                responseJson.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ListProperties));
            Assert.That(
                responseJson.RootElement.GetProperty("organizations")[0]
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EquivalentTo(OrganizationProperties));
            Assert.That(body!.ReasonCode, Is.EqualTo("organizations_listed"));
            Assert.That(
                body.Organizations.Select(organization => organization.OrganizationId),
                Is.EqualTo(new[] { first.OrganizationId, second.OrganizationId }));
        });
        AssertOrganization(
            body!.Organizations[0],
            first,
            "First Organization",
            SignupTime.AddDays(1));
        AssertOrganization(
            body.Organizations[1],
            second,
            "Second Organization",
            SignupTime.AddDays(2),
            productSeatAssigned: false);
    }

    [Test]
    public async Task AuthenticatedUserWithoutOrganizationsGetsEmptyList()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-no-organizations",
            "api-no-organizations@example.com");
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync("/api/organizations");
        var body = JsonSerializer.Deserialize<OrganizationListResponse>(
            await response.Content.ReadAsStringAsync(),
            WebJson);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(body!.ReasonCode, Is.EqualTo("organizations_listed"));
            Assert.That(body.Organizations, Is.Empty);
        });
    }

    [Test]
    public async Task OtherUsersOrganizationsAreNotExposed()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "api-listed-owner",
            "api-listed-owner@example.com");
        var other = await SignUpAsync(
            database,
            "api-listing-other",
            "api-listing-other@example.com");
        await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Private Organization",
            SignupTime.AddDays(1));
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(other.UserId);

        using var response = await client.GetAsync("/api/organizations");
        var body = JsonSerializer.Deserialize<OrganizationListResponse>(
            await response.Content.ReadAsStringAsync(),
            WebJson);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(body!.Organizations, Is.Empty);
        });
    }

    [TestCase(OrganizationRole.Admin, "admin", 5)]
    [TestCase(OrganizationRole.Member, "member", 7)]
    public async Task ListingProjectsTheMembersRoleAndAssignedSeat(
        OrganizationRole role,
        string expectedRole,
        int deviceLimit)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"api-role-owner-{expectedRole}",
            $"api-role-owner-{expectedRole}@example.com");
        var user = await SignUpAsync(
            database,
            $"api-role-{expectedRole}",
            $"api-role-{expectedRole}@example.com");
        var joinedAt = SignupTime.AddDays(2);
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Shared Organization",
            SignupTime.AddDays(1));
        var membershipId = Guid.CreateVersion7();
        var seatId = Guid.CreateVersion7();
        await AddOrganizationMemberAsync(
            database,
            organization,
            user.UserId,
            role,
            membershipId,
            seatId,
            deviceLimit,
            joinedAt);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(user.UserId);

        using var response = await client.GetAsync("/api/organizations");
        var body = JsonSerializer.Deserialize<OrganizationListResponse>(
            await response.Content.ReadAsStringAsync(),
            WebJson);
        var listed = body!.Organizations.Single();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(listed.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(listed.BillingAccountId, Is.EqualTo(organization.BillingAccountId));
            Assert.That(listed.MembershipId, Is.EqualTo(membershipId));
            Assert.That(listed.SeatId, Is.EqualTo(seatId));
            Assert.That(listed.SeatId, Is.Not.EqualTo(organization.SeatId));
            Assert.That(listed.Role, Is.EqualTo(expectedRole));
            Assert.That(listed.DeviceLimit, Is.EqualTo(deviceLimit));
            Assert.That(listed.JoinedAt, Is.EqualTo(joinedAt));
        });
    }

    [Test]
    public async Task EqualMembershipTimesUseTheMembershipIdentifierTieBreak()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-organization-tie",
            "api-organization-tie@example.com");
        var joinedAt = SignupTime.AddDays(1);
        var first = await CreateOrganizationAsync(
            database,
            signup.UserId,
            "Alpha Created First",
            joinedAt);
        var second = await CreateOrganizationAsync(
            database,
            signup.UserId,
            "Zulu Created Second",
            joinedAt);
        var lowerMembershipId = Guid.Parse("018f0000-0000-7000-8000-000000000001");
        var higherMembershipId = Guid.Parse("018f0000-0000-7000-8000-000000000002");
        await using (var updateContext = database.CreateContext())
        {
            await updateContext.OrganizationMemberships
                .Where(membership => membership.Id == first.OwnerMembershipId)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        membership => membership.Id,
                        higherMembershipId));
            await updateContext.OrganizationMemberships
                .Where(membership => membership.Id == second.OwnerMembershipId)
                .ExecuteUpdateAsync(
                    setters => setters.SetProperty(
                        membership => membership.Id,
                        lowerMembershipId));
        }

        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(signup.UserId);

        using var response = await client.GetAsync("/api/organizations");
        var body = JsonSerializer.Deserialize<OrganizationListResponse>(
            await response.Content.ReadAsStringAsync(),
            WebJson);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(
                body!.Organizations.Select(organization => organization.OrganizationId),
                Is.EqualTo(new[] { second.OrganizationId, first.OrganizationId }));
            Assert.That(
                body.Organizations.Select(organization => organization.MembershipId),
                Is.EqualTo(new[] { lowerMembershipId, higherMembershipId }));
        });
    }

    [Test]
    public async Task StaleAuthenticatedUserIsRejectedWithoutOrganizationDetails()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient(Guid.CreateVersion7());

        using var response = await client.GetAsync("/api/organizations");

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(response.Content.Headers.ContentLength, Is.EqualTo(0));
        });
    }

    [TestCase(null)]
    [TestCase("")]
    [TestCase(TestIdentifiers.MalformedText)]
    public async Task MissingOrInvalidAuthenticationIsChallengedWithoutRedirect(string? userId)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var factory = new LicensingWebApplicationFactory(database, SignupTime);
        using var client = factory.CreateApiClient();
        if (userId is not null)
        {
            client.DefaultRequestHeaders.Add(
                LicensingWebApplicationFactory.UserIdHeader,
                userId);
        }

        using var response = await client.GetAsync("/api/organizations");

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(response.Headers.Location, Is.Null);
        });
    }

    [Test]
    public async Task ClientCancellationStopsTheServerSideOrganizationQuery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpAsync(
            database,
            "api-organization-cancel",
            "api-organization-cancel@example.com");
        await CreateOrganizationAsync(
            database,
            signup.UserId,
            "Canceled Organization",
            SignupTime.AddDays(1));
        var gate = new DatabaseCommandGate();
        const string path = "/api/organizations";
        var requestCompletionObserver = new RequestCompletionObserver(HttpMethods.Get, path);
        var queryGateInterceptor = new DatabaseCommandGateInterceptor(
            gate,
            "FROM organization_memberships");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2),
            useTestAuthentication: true,
            requestCompletionObserver: requestCompletionObserver,
            interceptors: [queryGateInterceptor]);
        using var client = factory.CreateApiClient(signup.UserId);
        using var cancellation = new CancellationTokenSource();

        var responseTask = client.GetAsync(path, cancellation.Token);
        await gate.WaitUntilReachedAsync();
        try
        {
            cancellation.Cancel();
            Assert.ThrowsAsync<TaskCanceledException>(async () => await responseTask);
            await queryGateInterceptor.WaitUntilCancellationObservedAsync();
            await requestCompletionObserver.WaitUntilCompletedAsync();
        }
        finally
        {
            gate.Release();
        }

        Assert.That(requestCompletionObserver.WasCanceled, Is.True);
    }

    private static void AssertOrganization(
        OrganizationResponse organization,
        OrganizationCreationResult creation,
        string name,
        DateTimeOffset joinedAt,
        bool productSeatAssigned = true)
    {
        Assert.Multiple(() =>
        {
            Assert.That(organization.OrganizationId, Is.EqualTo(creation.OrganizationId));
            Assert.That(organization.BillingAccountId, Is.EqualTo(creation.BillingAccountId));
            Assert.That(organization.MembershipId, Is.EqualTo(creation.OwnerMembershipId));
            Assert.That(organization.SeatId, Is.EqualTo(creation.SeatId));
            Assert.That(organization.Name, Is.EqualTo(name));
            Assert.That(organization.Role, Is.EqualTo("owner"));
            Assert.That(organization.ProductSeatAssigned, Is.EqualTo(productSeatAssigned));
            Assert.That(organization.DeviceLimit, Is.EqualTo(3));
            Assert.That(organization.JoinedAt, Is.EqualTo(joinedAt));
            Assert.That(
                new[]
                {
                    organization.OrganizationId,
                    organization.BillingAccountId,
                    organization.MembershipId,
                    organization.SeatId,
                },
                Has.All.Property(nameof(Guid.Version)).EqualTo(7));
        });
    }

    private sealed record OrganizationListResponse(
        string ReasonCode,
        IReadOnlyList<OrganizationResponse> Organizations);

    private sealed record OrganizationResponse(
        Guid OrganizationId,
        Guid BillingAccountId,
        Guid MembershipId,
        Guid SeatId,
        string Name,
        string Role,
        bool ProductSeatAssigned,
        int DeviceLimit,
        DateTimeOffset JoinedAt);
}
