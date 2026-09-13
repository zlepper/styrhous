using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Api.Organizations;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationRoleManagementApiTests
{
    private static readonly string[] RoleChangeProperties =
    [
        "reasonCode",
        "organizationId",
        "membershipId",
        "userId",
        "previousRole",
        "role",
        "correlationId",
        "changedAt",
    ];

    private static readonly string[] OwnershipTransferProperties =
    [
        "reasonCode",
        "organizationId",
        "previousOwnerMembershipId",
        "previousOwnerUserId",
        "ownerMembershipId",
        "ownerUserId",
        "correlationId",
        "transferredAt",
    ];

    [Test]
    public async Task OwnerChangesRoleAndTransfersOwnershipThroughInternalApi()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "roles-api-owner", "owner@example.com");
        var target = await SignUpAsync(database, "roles-api-target", "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Role API Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Member);
        var observedAt = SignupTime.AddDays(2);
        using var factory = new LicensingWebApplicationFactory(database, observedAt);
        using var client = factory.CreateApiClient(owner.UserId);

        using var roleResponse = await ChangeRoleAsync(
            client,
            organization.OrganizationId,
            targetSetup.MembershipId,
            " ADMIN ");
        var roleText = await roleResponse.Content.ReadAsStringAsync();
        using var roleBody = JsonDocument.Parse(roleText);
        var roleCorrelationId = roleBody.RootElement.GetProperty("correlationId").GetGuid();
        using var transferResponse = await TransferOwnershipAsync(
            client,
            organization.OrganizationId,
            targetSetup.MembershipId);
        var transferText = await transferResponse.Content.ReadAsStringAsync();
        using var transferBody = JsonDocument.Parse(transferText);
        var transferCorrelationId = transferBody.RootElement
            .GetProperty("correlationId")
            .GetGuid();

        Assert.Multiple(() =>
        {
            Assert.That(roleResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(roleResponse.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(roleBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo(OrganizationReasonCodes.MemberRoleChanged));
            Assert.That(roleBody.RootElement.GetProperty("organizationId").GetGuid(),
                Is.EqualTo(organization.OrganizationId));
            Assert.That(roleBody.RootElement.GetProperty("membershipId").GetGuid(),
                Is.EqualTo(targetSetup.MembershipId));
            Assert.That(roleBody.RootElement.GetProperty("userId").GetGuid(),
                Is.EqualTo(target.UserId));
            Assert.That(roleBody.RootElement.GetProperty("previousRole").GetString(),
                Is.EqualTo("member"));
            Assert.That(roleBody.RootElement.GetProperty("role").GetString(),
                Is.EqualTo("admin"));
            Assert.That(roleCorrelationId.Version, Is.EqualTo(7));
            Assert.That(roleBody.RootElement.GetProperty("changedAt").GetDateTimeOffset(),
                Is.EqualTo(observedAt));
            Assert.That(
                roleBody.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(RoleChangeProperties));

            Assert.That(transferResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(transferResponse.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(transferBody.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo(OrganizationReasonCodes.OwnershipTransferred));
            Assert.That(transferBody.RootElement.GetProperty("organizationId").GetGuid(),
                Is.EqualTo(organization.OrganizationId));
            Assert.That(
                transferBody.RootElement.GetProperty("previousOwnerMembershipId").GetGuid(),
                Is.EqualTo(organization.OwnerMembershipId));
            Assert.That(transferBody.RootElement.GetProperty("previousOwnerUserId").GetGuid(),
                Is.EqualTo(owner.UserId));
            Assert.That(transferBody.RootElement.GetProperty("ownerMembershipId").GetGuid(),
                Is.EqualTo(targetSetup.MembershipId));
            Assert.That(transferBody.RootElement.GetProperty("ownerUserId").GetGuid(),
                Is.EqualTo(target.UserId));
            Assert.That(transferCorrelationId.Version, Is.EqualTo(7));
            Assert.That(transferCorrelationId, Is.Not.EqualTo(roleCorrelationId));
            Assert.That(
                transferBody.RootElement.GetProperty("transferredAt").GetDateTimeOffset(),
                Is.EqualTo(observedAt));
            Assert.That(
                transferBody.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(OwnershipTransferProperties));
        });

        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                context.OrganizationMemberships.Single(
                    membership => membership.Id == organization.OwnerMembershipId).Role,
                Is.EqualTo(OrganizationRole.Admin));
            Assert.That(
                context.OrganizationMemberships.Single(
                    membership => membership.Id == targetSetup.MembershipId).Role,
                Is.EqualTo(OrganizationRole.Owner));
            Assert.That(
                context.AuditRecords.Count(record =>
                    record.CorrelationId == roleCorrelationId
                    && record.Action == AuditAction.OrganizationMemberRoleChanged),
                Is.EqualTo(1));
            Assert.That(
                context.AuditRecords.Count(record =>
                    record.CorrelationId == transferCorrelationId),
                Is.EqualTo(2));
        });
    }

    [Test]
    public async Task RoleMutationsRequireAuthenticationAndAntiforgery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "roles-auth-owner", "owner@example.com");
        var target = await SignUpAsync(database, "roles-auth-target", "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Role Auth Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Member);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var anonymousClient = factory.CreateApiClient();
        using var staleClient = factory.CreateApiClient(Guid.CreateVersion7());
        using var ownerClient = factory.CreateApiClient(owner.UserId);

        using var anonymousResponse = await ChangeRoleWithoutAntiforgeryAsync(
            anonymousClient,
            organization.OrganizationId,
            targetSetup.MembershipId,
            "admin");
        using var staleResponse = await ChangeRoleAsync(
            staleClient,
            organization.OrganizationId,
            targetSetup.MembershipId,
            "admin");
        using var anonymousTransfer = await anonymousClient.PostAsync(
            TransferPath(organization.OrganizationId, targetSetup.MembershipId),
            content: null);
        using var staleTransfer = await TransferOwnershipAsync(
            staleClient,
            organization.OrganizationId,
            targetSetup.MembershipId);
        using var missingRoleToken = await ChangeRoleWithoutAntiforgeryAsync(
            ownerClient,
            organization.OrganizationId,
            targetSetup.MembershipId,
            "admin");
        using var missingTransferToken = await ownerClient.PostAsync(
            TransferPath(organization.OrganizationId, targetSetup.MembershipId),
            content: null);

        Assert.Multiple(() =>
        {
            Assert.That(anonymousResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(anonymousTransfer.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleTransfer.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(missingRoleToken.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(missingTransferToken.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
        });
        await using var context = database.CreateContext();
        Assert.That(
            context.OrganizationMemberships.Single(
                membership => membership.Id == targetSetup.MembershipId).Role,
            Is.EqualTo(OrganizationRole.Member));
    }

    [TestCase(null)]
    [TestCase("")]
    [TestCase("owner")]
    [TestCase("super-admin")]
    public async Task RoleChangeValidatesRequestedRole(string? role)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, $"invalid-role-{role}", "owner@example.com");
        var target = await SignUpAsync(database, $"invalid-target-{role}", "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invalid Role Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Member);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await ChangeRoleAsync(
            client,
            organization.OrganizationId,
            targetSetup.MembershipId,
            role);
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(body.RootElement.GetProperty("errors").TryGetProperty("role", out _),
                Is.True);
        });
        await using var context = database.CreateContext();
        Assert.That(
            context.OrganizationMemberships.Single(
                membership => membership.Id == targetSetup.MembershipId).Role,
            Is.EqualTo(OrganizationRole.Member));
    }

    [Test]
    public async Task HiddenOrganizationsAndUnknownMembersUsePrivateNotFoundResponses()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "roles-private-owner", "owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "roles-private-outsider",
            "outsider@example.com");
        var target = await SignUpAsync(database, "roles-private-target", "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Role Private Organization",
            SignupTime.AddDays(1));
        var otherOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Other Role Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Member);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var outsiderClient = factory.CreateApiClient(outsider.UserId);
        using var ownerClient = factory.CreateApiClient(owner.UserId);

        using var hiddenRole = await ChangeRoleAsync(
            outsiderClient,
            organization.OrganizationId.ToString(),
            TestIdentifiers.MalformedText,
            "admin");
        using var hiddenTransfer = await TransferOwnershipAsync(
            outsiderClient,
            organization.OrganizationId.ToString(),
            targetSetup.MembershipId.ToString());
        using var unknownRole = await ChangeRoleAsync(
            ownerClient,
            organization.OrganizationId.ToString(),
            TestIdentifiers.MalformedText,
            "admin");
        using var unknownTransfer = await TransferOwnershipAsync(
            ownerClient,
            organization.OrganizationId.ToString(),
            Guid.CreateVersion7().ToString());
        using var invalidOrganization = await ChangeRoleAsync(
            ownerClient,
            Guid.NewGuid().ToString(),
            targetSetup.MembershipId.ToString(),
            "admin");
        using var crossOrganization = await TransferOwnershipAsync(
            ownerClient,
            otherOrganization.OrganizationId.ToString(),
            targetSetup.MembershipId.ToString());
        var reasonCodes = await Task.WhenAll(
            ReadReasonCodeAsync(hiddenRole),
            ReadReasonCodeAsync(hiddenTransfer),
            ReadReasonCodeAsync(unknownRole),
            ReadReasonCodeAsync(unknownTransfer),
            ReadReasonCodeAsync(invalidOrganization),
            ReadReasonCodeAsync(crossOrganization));

        Assert.Multiple(() =>
        {
            AssertError(hiddenRole, reasonCodes[0], HttpStatusCode.NotFound,
                OrganizationReasonCodes.OrganizationNotFound);
            AssertError(hiddenTransfer, reasonCodes[1], HttpStatusCode.NotFound,
                OrganizationReasonCodes.OrganizationNotFound);
            AssertError(unknownRole, reasonCodes[2], HttpStatusCode.NotFound,
                OrganizationReasonCodes.MemberNotFound);
            AssertError(unknownTransfer, reasonCodes[3], HttpStatusCode.NotFound,
                OrganizationReasonCodes.MemberNotFound);
            AssertError(invalidOrganization, reasonCodes[4], HttpStatusCode.NotFound,
                OrganizationReasonCodes.OrganizationNotFound);
            AssertError(crossOrganization, reasonCodes[5], HttpStatusCode.NotFound,
                OrganizationReasonCodes.MemberNotFound);
        });
    }

    [Test]
    public async Task PermissionAndConflictResultsHaveStableReasonCodes()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "roles-errors-owner", "owner@example.com");
        var admin = await SignUpAsync(database, "roles-errors-admin", "admin@example.com");
        var member = await SignUpAsync(database, "roles-errors-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Role Errors Organization",
            SignupTime.AddDays(1));
        var adminSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin);
        var memberSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var adminClient = factory.CreateApiClient(admin.UserId);

        using var unauthorized = await ChangeRoleAsync(
            adminClient,
            organization.OrganizationId,
            memberSetup.MembershipId,
            "admin");
        using var unauthorizedOwnerChange = await ChangeRoleAsync(
            adminClient,
            organization.OrganizationId,
            organization.OwnerMembershipId,
            "member");
        using var unchanged = await ChangeRoleAsync(
            ownerClient,
            organization.OrganizationId,
            adminSetup.MembershipId,
            "admin");
        using var ownerChange = await ChangeRoleAsync(
            ownerClient,
            organization.OrganizationId,
            organization.OwnerMembershipId,
            "member");
        using var invalidTransfer = await TransferOwnershipAsync(
            ownerClient,
            organization.OrganizationId,
            organization.OwnerMembershipId);
        var reasonCodes = await Task.WhenAll(
            ReadReasonCodeAsync(unauthorized),
            ReadReasonCodeAsync(unauthorizedOwnerChange),
            ReadReasonCodeAsync(unchanged),
            ReadReasonCodeAsync(ownerChange),
            ReadReasonCodeAsync(invalidTransfer));

        Assert.Multiple(() =>
        {
            AssertError(unauthorized, reasonCodes[0], HttpStatusCode.Forbidden,
                OrganizationReasonCodes.InsufficientPermission);
            AssertError(
                unauthorizedOwnerChange,
                reasonCodes[1],
                HttpStatusCode.Forbidden,
                OrganizationReasonCodes.InsufficientPermission);
            AssertError(unchanged, reasonCodes[2], HttpStatusCode.Conflict,
                OrganizationReasonCodes.MemberRoleUnchanged);
            AssertError(ownerChange, reasonCodes[3], HttpStatusCode.Conflict,
                OrganizationReasonCodes.OwnershipTransferRequired);
            AssertError(invalidTransfer, reasonCodes[4], HttpStatusCode.Conflict,
                OrganizationReasonCodes.InvalidOwnershipTarget);
        });
    }

    [Test]
    public async Task RepeatedOptimisticConflictsReturnStableApiConflict()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "roles-race-owner", "owner@example.com");
        var target = await SignUpAsync(database, "roles-race-target", "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Role Race Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Member);
        var interceptor = new OrganizationRoleUpdateConflictInterceptor();
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2),
            useTestAuthentication: true,
            requestCompletionObserver: null,
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await ChangeRoleAsync(
            client,
            organization.OrganizationId,
            targetSetup.MembershipId,
            "admin");
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            AssertError(
                response,
                reasonCode,
                HttpStatusCode.Conflict,
                OrganizationReasonCodes.MembersChanged);
            Assert.That(interceptor.SuppressedUpdateCount, Is.EqualTo(3));
        });
    }

    [Test]
    public async Task RepeatedTransferTargetConflictsReturnStableApiConflict()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "transfer-race-owner",
            "owner@example.com");
        var target = await SignUpAsync(
            database,
            "transfer-race-target",
            "target@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Transfer Race Organization",
            SignupTime.AddDays(1));
        var targetSetup = await AddOrganizationMemberAsync(
            database,
            organization,
            target.UserId,
            OrganizationRole.Admin);
        var interceptor = new OrganizationRoleUpdateConflictInterceptor(
            suppressEveryUpdate: false);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2),
            useTestAuthentication: true,
            requestCompletionObserver: null,
            interceptors: [interceptor]);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await TransferOwnershipAsync(
            client,
            organization.OrganizationId,
            targetSetup.MembershipId);
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            AssertError(
                response,
                reasonCode,
                HttpStatusCode.Conflict,
                OrganizationReasonCodes.MembersChanged);
            Assert.That(interceptor.UpdateCount, Is.EqualTo(6));
            Assert.That(interceptor.SuppressedUpdateCount, Is.EqualTo(3));
        });
        await using var context = database.CreateContext();
        Assert.Multiple(() =>
        {
            Assert.That(
                context.OrganizationMemberships.Single(
                    membership => membership.Id == organization.OwnerMembershipId).Role,
                Is.EqualTo(OrganizationRole.Owner));
            Assert.That(
                context.OrganizationMemberships.Single(
                    membership => membership.Id == targetSetup.MembershipId).Role,
                Is.EqualTo(OrganizationRole.Admin));
        });
    }

    private static async Task<HttpResponseMessage> ChangeRoleAsync(
        HttpClient client,
        Guid organizationId,
        Guid membershipId,
        string? role)
    {
        return await ChangeRoleAsync(
            client,
            organizationId.ToString(),
            membershipId.ToString(),
            role);
    }

    private static async Task<HttpResponseMessage> ChangeRoleAsync(
        HttpClient client,
        string organizationId,
        string membershipId,
        string? role)
    {
        var request = new HttpRequestMessage(
            HttpMethod.Patch,
            RolePath(organizationId, membershipId))
        {
            Content = JsonContent.Create(new { role }),
        };
        AntiforgeryTestClient.AddToken(
            request,
            await AntiforgeryTestClient.GetTokenAsync(client));
        return await client.SendAsync(request);
    }

    private static Task<HttpResponseMessage> ChangeRoleWithoutAntiforgeryAsync(
        HttpClient client,
        Guid organizationId,
        Guid membershipId,
        string? role)
    {
        return client.SendAsync(
            new HttpRequestMessage(
                HttpMethod.Patch,
                RolePath(organizationId.ToString(), membershipId.ToString()))
            {
                Content = JsonContent.Create(new { role }),
            });
    }

    private static async Task<HttpResponseMessage> TransferOwnershipAsync(
        HttpClient client,
        Guid organizationId,
        Guid membershipId)
    {
        return await TransferOwnershipAsync(
            client,
            organizationId.ToString(),
            membershipId.ToString());
    }

    private static async Task<HttpResponseMessage> TransferOwnershipAsync(
        HttpClient client,
        string organizationId,
        string membershipId)
    {
        var request = new HttpRequestMessage(
            HttpMethod.Post,
            TransferPath(organizationId, membershipId));
        AntiforgeryTestClient.AddToken(
            request,
            await AntiforgeryTestClient.GetTokenAsync(client));
        return await client.SendAsync(request);
    }

    private static string RolePath(string organizationId, string membershipId)
    {
        return $"/api/organizations/{organizationId}/members/{membershipId}/role";
    }

    private static string TransferPath(Guid organizationId, Guid membershipId)
    {
        return TransferPath(organizationId.ToString(), membershipId.ToString());
    }

    private static string TransferPath(string organizationId, string membershipId)
    {
        return $"/api/organizations/{organizationId}/members/{membershipId}/transfer-ownership";
    }

    private static void AssertError(
        HttpResponseMessage response,
        string? actualReasonCode,
        HttpStatusCode statusCode,
        string reasonCode)
    {
        Assert.That(response.StatusCode, Is.EqualTo(statusCode));
        Assert.That(actualReasonCode, Is.EqualTo(reasonCode));
        Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
    }

    private static async Task<string?> ReadReasonCodeAsync(HttpResponseMessage response)
    {
        using var body = JsonDocument.Parse(await response.Content.ReadAsStringAsync());
        return body.RootElement.GetProperty("reasonCode").GetString();
    }
}
