using System.Net;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationListingApiTests
{
    private static readonly JsonSerializerOptions WebJson =
        new(JsonSerializerDefaults.Web);

    private static readonly string[] ResponseProperties =
    [
        "reasonCode",
        "organizationId",
        "invitations",
    ];

    private static readonly string[] InvitationProperties =
    [
        "invitationId",
        "createdByUserId",
        "email",
        "role",
        "assignProductSeat",
        "createdAt",
        "lastSentAt",
        "expiresAt",
    ];

    private static readonly string[] ErrorResponseProperties = ["reasonCode"];

    [Test]
    public async Task OwnersAndAdminsListCompleteSafeInvitationProjection()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "invitation-list-api-owner",
            "owner@example.com");
        var admin = await SignUpAsync(
            database,
            "invitation-list-api-admin",
            "admin@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Listing API Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            admin.UserId,
            OrganizationRole.Admin);
        var firstInvitation = await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "first-api@example.com",
            SignupTime.AddDays(2),
            OrganizationRole.Admin);
        var secondInvitation = await CreateInvitationAsync(
            database,
            admin.UserId,
            organization.OrganizationId,
            "second-api@example.com",
            SignupTime.AddDays(3),
            assignProductSeat: false);
        var firstResentAt = SignupTime.AddDays(4);
        var resend = await ResendInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            firstInvitation.InvitationId,
            firstResentAt);
        var otherOrganization = await CreateOrganizationAsync(
            database,
            admin.UserId,
            "Other Invitation Listing API Organization",
            SignupTime.AddDays(4));
        var otherInvitation = await CreateInvitationAsync(
            database,
            admin.UserId,
            otherOrganization.OrganizationId,
            "other-api@example.com",
            SignupTime.AddDays(5));
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(6));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var adminClient = factory.CreateApiClient(admin.UserId);

        using var ownerResponse = await ownerClient.GetAsync(
            InvitationPath(organization.OrganizationId));
        using var adminResponse = await adminClient.GetAsync(
            InvitationPath(organization.OrganizationId));
        var ownerBodyText = await ownerResponse.Content.ReadAsStringAsync();
        var adminBodyText = await adminResponse.Content.ReadAsStringAsync();
        var body = JsonSerializer.Deserialize<InvitationListResponse>(ownerBodyText, WebJson);
        using var bodyJson = JsonDocument.Parse(ownerBodyText);

        Assert.Multiple(() =>
        {
            Assert.That(ownerResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(adminResponse.StatusCode, Is.EqualTo(HttpStatusCode.OK));
            Assert.That(adminBodyText, Is.EqualTo(ownerBodyText));
            Assert.That(ownerResponse.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                ownerResponse.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/json"));
            Assert.That(body!.ReasonCode, Is.EqualTo("organization_invitations_listed"));
            Assert.That(body.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(
                body.Invitations.Select(invitation => invitation.InvitationId),
                Is.EqualTo(new[]
                {
                    firstInvitation.InvitationId,
                    secondInvitation.InvitationId,
                }));
            Assert.That(
                body.Invitations.Select(invitation => invitation.InvitationId),
                Does.Not.Contain(otherInvitation.InvitationId));
            Assert.That(
                bodyJson.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ResponseProperties));
            Assert.That(
                bodyJson.RootElement.GetProperty("invitations")[0]
                    .EnumerateObject()
                    .Select(property => property.Name),
                Is.EquivalentTo(InvitationProperties));
            Assert.That(ownerBodyText, Does.Not.Contain(firstInvitation.Secret.Reveal()));
            Assert.That(ownerBodyText, Does.Not.Contain(secondInvitation.Secret.Reveal()));
            Assert.That(ownerBodyText, Does.Not.Contain(resend.Secret.Reveal()));
            Assert.That(ownerBodyText, Does.Not.Contain("other-api@example.com"));
        });
        AssertInvitation(
            body!.Invitations[0],
            firstInvitation.InvitationId,
            owner.UserId,
            "first-api@example.com",
            "admin",
            SignupTime.AddDays(2),
            lastSentAt: firstResentAt);
        AssertInvitation(
            body.Invitations[1],
            secondInvitation.InvitationId,
            admin.UserId,
            "second-api@example.com",
            "member",
            SignupTime.AddDays(3),
            assignProductSeat: false);
    }

    [Test]
    public async Task MemberIsForbiddenWhileHiddenAndUnknownOrganizationsMatch()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "invitation-private-api-owner",
            "owner@example.com");
        var member = await SignUpAsync(
            database,
            "invitation-private-api-member",
            "member@example.com");
        var outsider = await SignUpAsync(
            database,
            "invitation-private-api-outsider",
            "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Private Invitation API Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        await CreateOrganizationAsync(
            database,
            outsider.UserId,
            "Outsider Invitation API Organization",
            SignupTime.AddDays(1));
        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "private-api@example.com",
            SignupTime.AddDays(2));
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var memberClient = factory.CreateApiClient(member.UserId);
        using var outsiderClient = factory.CreateApiClient(outsider.UserId);

        using var memberResponse = await memberClient.GetAsync(
            InvitationPath(organization.OrganizationId));
        using var hiddenResponse = await outsiderClient.GetAsync(
            InvitationPath(organization.OrganizationId));
        using var unknownResponse = await ownerClient.GetAsync(
            InvitationPath(Guid.CreateVersion7()));
        using var versionFourResponse = await ownerClient.GetAsync(
            InvitationPath(Guid.NewGuid()));
        using var malformedResponse = await ownerClient.GetAsync(
            InvitationPath(TestIdentifiers.MalformedText));
        using var nonRfcVariantResponse = await ownerClient.GetAsync(
            InvitationPath(TestIdentifiers.Version7WithNonRfcVariant));
        var hiddenResponses = new[]
        {
            hiddenResponse,
            unknownResponse,
            versionFourResponse,
            malformedResponse,
            nonRfcVariantResponse,
        };
        var hiddenBodies = await Task.WhenAll(
            hiddenResponses.Select(response => response.Content.ReadAsStringAsync()));
        var memberBody = await memberResponse.Content.ReadAsStringAsync();

        Assert.Multiple(() =>
        {
            Assert.That(memberResponse.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(memberResponse.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(
                hiddenResponses.Select(response => response.StatusCode),
                Is.All.EqualTo(HttpStatusCode.NotFound));
            Assert.That(hiddenBodies.Skip(1), Is.All.EqualTo(hiddenBodies[0]));
            Assert.That(
                hiddenResponses.Select(response => response.Headers.CacheControl?.NoStore),
                Is.All.True);
        });
        AssertError(memberBody, "insufficient_permission");
        foreach (var hiddenBody in hiddenBodies)
        {
            AssertError(hiddenBody, "organization_not_found");
        }
    }

    [Test]
    public async Task AnonymousAndStaleUsersCannotListInvitations()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "invitation-auth-api-owner",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Auth API Organization",
            SignupTime.AddDays(1));
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2));
        using var anonymousClient = factory.CreateApiClient();
        using var staleClient = factory.CreateApiClient(Guid.CreateVersion7());

        using var anonymousResponse = await anonymousClient.GetAsync(
            InvitationPath(organization.OrganizationId));
        using var staleResponse = await staleClient.GetAsync(
            InvitationPath(organization.OrganizationId));

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
    public async Task ClientCancellationStopsServerSideInvitationQuery()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "invitation-cancel-api-owner",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Cancellation API Organization",
            SignupTime.AddDays(1));
        await CreateInvitationAsync(
            database,
            owner.UserId,
            organization.OrganizationId,
            "cancellation-api@example.com",
            SignupTime.AddDays(2));
        var path = InvitationPath(organization.OrganizationId);
        var gate = new DatabaseCommandGate();
        var completionObserver = new RequestCompletionObserver(HttpMethods.Get, path);
        var queryGateInterceptor = new DatabaseCommandGateInterceptor(
            gate,
            "FROM organization_invitations");
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(3),
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

    private static string InvitationPath(Guid organizationId)
    {
        return InvitationPath(organizationId.ToString());
    }

    private static string InvitationPath(string organizationId)
    {
        return $"/api/organizations/{organizationId}/invitations";
    }

    private static void AssertError(string bodyText, string reasonCode)
    {
        using var body = JsonDocument.Parse(bodyText);
        Assert.Multiple(() =>
        {
            Assert.That(
                body.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(ErrorResponseProperties));
            Assert.That(
                body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo(reasonCode));
        });
    }

    private static void AssertInvitation(
        InvitationResponse invitation,
        Guid invitationId,
        Guid createdByUserId,
        string email,
        string role,
        DateTimeOffset createdAt,
        bool assignProductSeat = true,
        DateTimeOffset? lastSentAt = null)
    {
        var expectedLastSentAt = lastSentAt ?? createdAt;
        Assert.Multiple(() =>
        {
            Assert.That(invitation.InvitationId, Is.EqualTo(invitationId));
            Assert.That(invitation.CreatedByUserId, Is.EqualTo(createdByUserId));
            Assert.That(invitation.Email, Is.EqualTo(email));
            Assert.That(invitation.Role, Is.EqualTo(role));
            Assert.That(invitation.AssignProductSeat, Is.EqualTo(assignProductSeat));
            Assert.That(invitation.CreatedAt, Is.EqualTo(createdAt));
            Assert.That(invitation.LastSentAt, Is.EqualTo(expectedLastSentAt));
            Assert.That(
                invitation.ExpiresAt,
                Is.EqualTo(expectedLastSentAt.Add(OrganizationInvitation.Lifetime)));
            Assert.That(invitation.InvitationId.Version, Is.EqualTo(7));
            Assert.That(invitation.CreatedByUserId.Version, Is.EqualTo(7));
        });
    }

    private sealed record InvitationListResponse(
        string ReasonCode,
        Guid OrganizationId,
        IReadOnlyList<InvitationResponse> Invitations);

    private sealed record InvitationResponse(
        Guid InvitationId,
        Guid CreatedByUserId,
        string Email,
        string Role,
        bool AssignProductSeat,
        DateTimeOffset CreatedAt,
        DateTimeOffset LastSentAt,
        DateTimeOffset ExpiresAt);
}
