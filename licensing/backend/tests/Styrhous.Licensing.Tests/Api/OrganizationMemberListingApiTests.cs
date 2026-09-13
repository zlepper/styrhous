using System.Net;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationMemberListingApiTests
{
    private static readonly JsonSerializerOptions WebJson =
        new(JsonSerializerDefaults.Web);

    private static readonly string[] ResponseProperties =
    [
        "reasonCode",
        "organizationId",
        "members",
    ];

    private static readonly string[] MemberProperties =
    [
        "membershipId",
        "userId",
        "seatId",
        "email",
        "role",
        "productSeatAssigned",
        "deviceLimit",
        "joinedAt",
    ];

    private static readonly string[] ErrorResponseProperties = ["reasonCode"];

    [Test]
    public async Task EveryRoleListsOrganizationRosterWithCompleteProjection()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "member-api-owner",
            "owner@example.com");
        var admin = await SignUpAsync(
            database,
            "member-api-admin",
            "admin@example.com");
        var member = await SignUpAsync(
            database,
            "member-api-member",
            "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Member API Organization",
            SignupTime.AddDays(1));
        var adminSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin,
            joinedAt: SignupTime.AddDays(2));
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member,
            productAccessEnabled: false,
            joinedAt: SignupTime.AddDays(3));
        var otherOrganization = await CreateOrganizationAsync(
            database,
            admin.UserId,
            "Other Member API Organization",
            SignupTime.AddDays(4));
        var pendingInvitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "pending-api-roster@example.com",
            SignupTime.AddDays(5));
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(6));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var adminClient = factory.CreateApiClient(admin.UserId);
        using var client = factory.CreateApiClient(member.UserId);

        using var ownerResponse = await ownerClient.GetAsync(
            MemberPath(organization.OrganizationId));
        using var adminResponse = await adminClient.GetAsync(
            MemberPath(organization.OrganizationId));
        using var response = await client.GetAsync(MemberPath(organization.OrganizationId));
        var bodyText = await response.Content.ReadAsStringAsync();
        var body = JsonSerializer.Deserialize<MemberListResponse>(bodyText, WebJson);
        using var bodyJson = JsonDocument.Parse(bodyText);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(ownerResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(adminResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                response.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/json"));
            Assert.That(body!.ReasonCode, Is.EqualTo("organization_members_listed"));
            Assert.That(body.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(body.Members, Has.Count.EqualTo(3));
            Assert.That(
                body.Members.Select(candidate => candidate.MembershipId),
                Is.EqualTo(new[]
                {
                    organization.OwnerMembershipId,
                    adminSetup.MembershipId,
                    memberSetup.MembershipId,
                }));
            Assert.That(
                body.Members.Select(candidate => candidate.MembershipId),
                Does.Not.Contain(otherOrganization.OwnerMembershipId));
            Assert.That(bodyText, Does.Not.Contain("pending-api-roster@example.com"));
            Assert.That(bodyText, Does.Not.Contain(pendingInvitation.InvitationId.ToString()));
            Assert.That(
                bodyJson.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ResponseProperties));
            Assert.That(
                bodyJson.RootElement.GetProperty("members")[0]
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EquivalentTo(MemberProperties));
        });
        AssertMember(
            body!.Members.Single(candidate => candidate.UserId == owner.UserId),
            organization.OwnerMembershipId,
            organization.SeatId,
            "owner@example.com",
            "owner",
            SignupTime.AddDays(1));
        AssertMember(
            body.Members.Single(candidate => candidate.UserId == admin.UserId),
            adminSetup.MembershipId,
            adminSetup.SeatId,
            "admin@example.com",
            "admin",
            SignupTime.AddDays(2));
        AssertMember(
            body.Members.Single(candidate => candidate.UserId == member.UserId),
            memberSetup.MembershipId,
            memberSetup.SeatId,
            "member@example.com",
            "member",
            SignupTime.AddDays(3),
            productSeatAssigned: false);
    }

    [Test]
    public async Task UnknownAndHiddenOrganizationsShareNotFoundRegardlessOfIdentifierVersion()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "member-private-api-owner",
            "owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "member-private-api-outsider",
            "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Private Member API Organization",
            SignupTime.AddDays(1));
        await CreateOrganizationAsync(
            database,
            outsider.UserId,
            "Outsider API Organization",
            SignupTime.AddDays(2));
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var outsiderClient = factory.CreateApiClient(outsider.UserId);

        using var unknownResponse = await ownerClient.GetAsync(MemberPath(Guid.CreateVersion7()));
        using var hiddenResponse = await outsiderClient.GetAsync(
            MemberPath(organization.OrganizationId));
        using var versionFourResponse = await ownerClient.GetAsync(MemberPath(Guid.NewGuid()));
        using var malformedResponse = await ownerClient.GetAsync(
            MemberPath(TestIdentifiers.MalformedText));
        using var nonRfcVariantResponse = await ownerClient.GetAsync(
            MemberPath(TestIdentifiers.Version7WithNonRfcVariant));
        var responses = new[]
        {
            unknownResponse,
            hiddenResponse,
            versionFourResponse,
            malformedResponse,
            nonRfcVariantResponse,
        };
        var responseBodies = await Task.WhenAll(
            responses.Select(response => response.Content.ReadAsStringAsync()));

        Assert.Multiple(() =>
        {
            Assert.That(
                responses.Select(response => response.StatusCode),
                Is.All.EqualTo(HttpStatusCode.NotFound));
            Assert.That(responseBodies.Skip(1), Is.All.EqualTo(responseBodies[0]));
            Assert.That(
                responses.Select(response => response.Headers.CacheControl?.NoStore),
                Is.All.True);
        });
        foreach (var responseBody in responseBodies)
        {
            using var body = JsonDocument.Parse(responseBody);
            Assert.Multiple(() =>
            {
                Assert.That(
                    body.RootElement.EnumerateObject().Select(property => property.Name),
                    Is.EquivalentTo(ErrorResponseProperties));
                Assert.That(
                    body.RootElement.GetProperty("reasonCode").GetString(),
                    Is.EqualTo("organization_not_found"));
            });
        }
    }

    [Test]
    public async Task AnonymousAndStaleUsersCannotListMembers()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "member-auth-api-owner",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Member Auth API Organization",
            SignupTime.AddDays(1));
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var anonymousClient = factory.CreateApiClient();
        using var staleClient = factory.CreateApiClient(Guid.CreateVersion7());

        using var anonymousResponse = await anonymousClient.GetAsync(
            MemberPath(organization.OrganizationId));
        using var staleResponse = await staleClient.GetAsync(
            MemberPath(organization.OrganizationId));

        Assert.Multiple(() =>
        {
            Assert.That(anonymousResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(anonymousResponse.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(staleResponse.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(anonymousResponse.Headers.Location, Is.Null);
            Assert.That(staleResponse.Headers.Location, Is.Null);
        });
    }

    [Test]
    public async Task ClientCancellationStopsServerSideMemberQuery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "member-cancel-api-owner",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Member Cancellation API Organization",
            SignupTime.AddDays(1));
        var path = MemberPath(organization.OrganizationId);
        var gate = new DatabaseCommandGate();
        var completionObserver = new RequestCompletionObserver(HttpMethods.Get, path);
        var queryGateInterceptor = new DatabaseCommandGateInterceptor(
            gate,
            "JOIN user_accounts");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2),
            useTestAuthentication: true,
            requestCompletionObserver: completionObserver,
            interceptors: [queryGateInterceptor]);
        using var client = factory.CreateApiClient(owner.UserId);
        using var cancellation = new CancellationTokenSource();

        var responseTask = client.GetAsync(path, cancellation.Token);
        await gate.WaitUntilReachedAsync();
        try
        {
            cancellation.Cancel();
            Assert.ThrowsAsync<TaskCanceledException>(async () => await responseTask);
            await queryGateInterceptor.WaitUntilCancellationObservedAsync();
            await completionObserver.WaitUntilCompletedAsync();
        }
        finally
        {
            gate.Release();
        }

        Assert.That(completionObserver.WasCanceled, Is.True);
    }

    private static string MemberPath(Guid organizationId)
    {
        return MemberPath(organizationId.ToString());
    }

    private static string MemberPath(string organizationId)
    {
        return $"/api/organizations/{organizationId}/members";
    }

    private static void AssertMember(
        MemberResponse member,
        Guid membershipId,
        Guid seatId,
        string email,
        string role,
        DateTimeOffset joinedAt,
        bool productSeatAssigned = true)
    {
        Assert.Multiple(() =>
        {
            Assert.That(member.MembershipId, Is.EqualTo(membershipId));
            Assert.That(member.SeatId, Is.EqualTo(seatId));
            Assert.That(member.Email, Is.EqualTo(email));
            Assert.That(member.Role, Is.EqualTo(role));
            Assert.That(member.ProductSeatAssigned, Is.EqualTo(productSeatAssigned));
            Assert.That(member.DeviceLimit, Is.EqualTo(3));
            Assert.That(member.JoinedAt, Is.EqualTo(joinedAt));
            Assert.That(
                new[] { member.MembershipId, member.UserId, member.SeatId },
                Has.All.Property(nameof(Guid.Version)).EqualTo(7));
        });
    }

    private sealed record MemberListResponse(
        string ReasonCode,
        Guid OrganizationId,
        IReadOnlyList<MemberResponse> Members);

    private sealed record MemberResponse(
        Guid MembershipId,
        Guid UserId,
        Guid SeatId,
        string Email,
        string Role,
        bool ProductSeatAssigned,
        int DeviceLimit,
        DateTimeOffset JoinedAt);
}
