using Styrhous.Licensing.Persistence;
using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Microsoft.AspNetCore.Http;
using Microsoft.EntityFrameworkCore;
using Styrhous.Licensing.Api.Organizations;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Tests.Infrastructure;
using Styrhous.Licensing.Tests.Persistence;
using static Styrhous.Licensing.Tests.Persistence.LicensingPersistenceScenario;

namespace Styrhous.Licensing.Tests.Api;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationInvitationApiTests
{
    private static readonly string[] CreatedResponseProperties =
    [
        "reasonCode",
        "invitationId",
        "correlationId",
        "expiresAt",
    ];

    [Test]
    public async Task OwnerCreatesInvitationWithoutExposingItsSecret()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-api-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation API Organization",
            SignupTime.AddDays(1));
        var observedAt = SignupTime.AddDays(2);
        using var factory = new LicensingWebApplicationFactory(
            database,
            observedAt);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(new { email = "Invitee@Example.com", role = "member" }));
        var bodyText = await response.Content.ReadAsStringAsync();
        using var body = JsonDocument.Parse(bodyText);
        var invitationId = body.RootElement.GetProperty("invitationId").GetGuid();
        var correlationId = body.RootElement.GetProperty("correlationId").GetGuid();

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Created));
            Assert.That(response.Headers.Location?.ToString(),
                Is.EqualTo(
                    $"/api/organizations/{organization.OrganizationId}/invitations/{invitationId}"));
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
            Assert.That(body.RootElement.GetProperty("reasonCode").GetString(),
                Is.EqualTo("invitation_created"));
            Assert.That(body.RootElement.GetProperty("expiresAt").GetDateTimeOffset(),
                Is.EqualTo(observedAt.AddDays(7)));
            Assert.That(invitationId.Version, Is.EqualTo(7));
            Assert.That(correlationId.Version, Is.EqualTo(7));
            Assert.That(
                body.RootElement.EnumerateObject().Select(property => property.Name),
                Is.EquivalentTo(CreatedResponseProperties));
            Assert.That(bodyText, Does.Not.Contain("secret").IgnoreCase);
        });

        await using var context = database.CreateContext();
        var invitation = await context.OrganizationInvitations.SingleAsync();
        var audit = await context.AuditRecords.SingleAsync(
            record => record.CorrelationId == correlationId);
        var outbox = await context.OutboxMessages.SingleAsync(
            message => message.CorrelationId == correlationId);
        Assert.Multiple(() =>
        {
            Assert.That(invitation.Id, Is.EqualTo(invitationId));
            Assert.That(invitation.OrganizationId, Is.EqualTo(organization.OrganizationId));
            Assert.That(invitation.CreatedByUserId, Is.EqualTo(owner.UserId));
            Assert.That(invitation.Email, Is.EqualTo("Invitee@Example.com"));
            Assert.That(invitation.NormalizedEmail, Is.EqualTo("INVITEE@EXAMPLE.COM"));
            Assert.That(invitation.Role, Is.EqualTo(OrganizationRole.Member));
            Assert.That(invitation.AssignProductSeat, Is.True);
            Assert.That(invitation.SecretHash, Has.Length.EqualTo(64));
            Assert.That(audit.Action, Is.EqualTo(AuditAction.OrganizationInvitationCreated));
            Assert.That(audit.TargetType, Is.EqualTo(AuditTargetType.OrganizationInvitation));
            Assert.That(audit.TargetId, Is.EqualTo(invitationId));
            Assert.That(outbox.SubjectId, Is.EqualTo(invitationId));
            Assert.That(
                outbox.MessageType,
                Is.EqualTo(OutboxMessageTypes.OrganizationInvitationDelivery));
            Assert.That(outbox.NotAfter, Is.EqualTo(observedAt.AddDays(7)));
            Assert.That(outbox.NativeOutboxEnqueued, Is.True);
            Assert.That(outbox.DiscardedAt, Is.Null);
            Assert.That(outbox.ProtectedPayload, Does.Not.Contain("Invitee@Example.com"));
            Assert.That(context.Set<RebusOutboxMessage>().Count(message =>
                message.DestinationAddress == database.DatabaseName), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task OwnerCreatesSeatlessInvitationWithoutLicensedCapacity()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "seatless-invitation-api-owner",
            "owner@example.com");
        await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Trial Holder",
            SignupTime.AddDays(1));
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Seatless Invitation API Organization",
            SignupTime.AddDays(2));
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(
                new
                {
                    email = "admin@example.com",
                    role = "admin",
                    assignProductSeat = false,
                }));

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Created));
        await using var context = database.CreateContext();
        var invitation = await context.OrganizationInvitations.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(invitation.AssignProductSeat, Is.False);
            Assert.That(invitation.ReservedSeatCapacity, Is.Zero);
        });
    }

    [Test]
    public async Task InvitationRemainsDurablyQueuedWhenForwardingIsDeferred()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            "invitation-hint-failure-owner",
            "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Deferred Invitation Delivery Organization",
            SignupTime.AddDays(1));
        var observedAt = SignupTime.AddDays(2);
        using var factory = new LicensingWebApplicationFactory(
            database,
            observedAt);
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(new { email = "invitee@example.com", role = "member" }));

        await using var context = database.CreateContext();
        var outbox = await context.OutboxMessages.AsNoTracking().SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Created));
            Assert.That(outbox.DiscardedAt, Is.Null);
            Assert.That(context.Set<RebusOutboxMessage>().Count(message =>
                message.DestinationAddress == database.DatabaseName), Is.EqualTo(1));
        });
    }

    [Test]
    public async Task UnauthenticatedAndStaleUsersCannotCreateInvitations()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-auth-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Auth Organization",
            SignupTime.AddDays(1));
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var anonymousClient = factory.CreateApiClient();
        using var staleClient = factory.CreateApiClient(Guid.CreateVersion7());

        using var anonymousResponse = await anonymousClient.PostAsJsonAsync(
            InvitationPath(organization.OrganizationId),
            new { email = "anonymous@example.com", role = "member" });
        using var staleResponse = await PostInvitationAsync(
            staleClient,
            organization.OrganizationId,
            JsonContent.Create(new { email = "stale@example.com", role = "member" }));

        Assert.Multiple(() =>
        {
            Assert.That(anonymousResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleResponse.StatusCode, Is.EqualTo(HttpStatusCode.Unauthorized));
            Assert.That(staleResponse.Headers.CacheControl?.NoStore, Is.True);
        });
        Assert.That(await CaptureWritesAsync(database), Is.EqualTo(baseline));
    }

    [Test]
    public async Task MissingAntiforgeryTokenRejectsInvitationWithoutWriting()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-csrf-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation CSRF Organization",
            SignupTime.AddDays(1));
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await client.PostAsJsonAsync(
            InvitationPath(organization.OrganizationId),
            new { email = "invitee@example.com", role = "member" });

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
        Assert.That(await CaptureWritesAsync(database), Is.EqualTo(baseline));
    }

    [TestCase("not-an-email", "member", "email")]
    [TestCase("invitee@example.com", "owner", "role")]
    [TestCase("invitee@example.com", "billing-admin", "role")]
    public async Task InvalidInvitationFieldsReturnValidationProblemWithoutWriting(
        string email,
        string role,
        string expectedField)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(
            database,
            $"invitation-validation-{Guid.CreateVersion7():N}",
            $"owner-{Guid.CreateVersion7():N}@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Validation Organization",
            SignupTime.AddDays(1));
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(new { email, role }));
        using var body = await JsonDocument.ParseAsync(
            await response.Content.ReadAsStreamAsync());

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(response.Content.Headers.ContentType?.MediaType,
                Is.EqualTo("application/problem+json"));
            Assert.That(body.RootElement.GetProperty("errors").TryGetProperty(
                    expectedField,
                    out _),
                Is.True);
            Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
        });
        Assert.That(await CaptureWritesAsync(database), Is.EqualTo(baseline));
    }

    [Test]
    public async Task MissingInvitationFieldsReturnBothValidationErrors()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-fields-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Fields Organization",
            SignupTime.AddDays(1));
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(new Dictionary<string, string>()));
        using var body = await JsonDocument.ParseAsync(
            await response.Content.ReadAsStreamAsync());
        var errors = body.RootElement.GetProperty("errors");

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.BadRequest));
            Assert.That(errors.TryGetProperty("email", out _), Is.True);
            Assert.That(errors.TryGetProperty("role", out _), Is.True);
        });
    }

    [Test]
    [SetCulture("tr-TR")]
    public async Task AdminRoleIsAcceptedCaseInsensitively()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-admin-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Admin Organization",
            SignupTime.AddDays(1));
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(owner.UserId);

        using var response = await PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(new { email = "admin@example.com", role = " ADMIN " }));

        Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Created));
        await using var context = database.CreateContext();
        Assert.That((await context.OrganizationInvitations.SingleAsync()).Role,
            Is.EqualTo(OrganizationRole.Admin));
    }

    [Test]
    public async Task InvalidUnknownAndHiddenOrganizationIdentifiersShareNotFoundResponse()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-private-owner", "owner@example.com");
        var outsider = await SignUpAsync(
            database,
            "invitation-private-outsider",
            "outsider@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Private Organization",
            SignupTime.AddDays(1));
        var baseline = await CaptureWritesAsync(database);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var ownerClient = factory.CreateApiClient(owner.UserId);
        using var outsiderClient = factory.CreateApiClient(outsider.UserId);

        using var invalidResponse = await PostInvitationAsync(
            ownerClient,
            Guid.NewGuid(),
            JsonContent.Create(new { email = "invalid@example.com", role = "member" }));
        using var malformedResponse = await PostInvitationAsync(
            ownerClient,
            TestIdentifiers.MalformedText,
            JsonContent.Create(new { email = "malformed@example.com", role = "member" }));
        using var nonRfcVariantResponse = await PostInvitationAsync(
            ownerClient,
            TestIdentifiers.Version7WithNonRfcVariant,
            JsonContent.Create(new { email = "non-rfc@example.com", role = "member" }));
        using var unknownResponse = await PostInvitationAsync(
            ownerClient,
            Guid.CreateVersion7(),
            JsonContent.Create(new { email = "unknown@example.com", role = "member" }));
        using var hiddenResponse = await PostInvitationAsync(
            outsiderClient,
            organization.OrganizationId,
            JsonContent.Create(new { email = "hidden@example.com", role = "member" }));

        var responses = new[]
        {
            invalidResponse,
            malformedResponse,
            nonRfcVariantResponse,
            unknownResponse,
            hiddenResponse,
        };
        var reasonCodes = await Task.WhenAll(responses.Select(ReadReasonCodeAsync));
        for (var index = 0; index < responses.Length; index++)
        {
            var response = responses[index];
            Assert.Multiple(() =>
            {
                Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.NotFound));
                Assert.That(response.Headers.CacheControl?.NoStore, Is.True);
                Assert.That(reasonCodes[index],
                    Is.EqualTo(OrganizationInvitationReasonCodes.OrganizationNotFound));
            });
        }

        Assert.That(await CaptureWritesAsync(database), Is.EqualTo(baseline));
    }

    [Test]
    public async Task MemberCannotCreateInvitation()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-member-owner", "owner@example.com");
        var member = await SignUpAsync(database, "invitation-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Member Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(member.UserId);

        using var response = await PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(new { email = "invitee@example.com", role = "member" }));
        var reasonCode = await ReadReasonCodeAsync(response);

        Assert.Multiple(() =>
        {
            Assert.That(response.StatusCode, Is.EqualTo(HttpStatusCode.Forbidden));
            Assert.That(reasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.InsufficientPermission));
        });
    }

    [Test]
    public async Task ExistingMemberAndPendingInvitationReturnSpecificConflicts()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-conflict-owner", "owner@example.com");
        var member = await SignUpAsync(database, "invitation-conflict-member", "member@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Conflict Organization",
            SignupTime.AddDays(1));
        await AddOrganizationMemberAsync(
            database,
            organization,
            member.UserId,
            OrganizationRole.Member);
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(2));
        using var client = factory.CreateApiClient(owner.UserId);

        using var memberResponse = await PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(new { email = "MEMBER@example.com", role = "admin" }));
        using var createdResponse = await PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(new { email = "pending@example.com", role = "member" }));
        using var duplicateResponse = await PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(new { email = "PENDING@example.com", role = "admin" }));
        var memberReasonCode = await ReadReasonCodeAsync(memberResponse);
        var duplicateReasonCode = await ReadReasonCodeAsync(duplicateResponse);

        Assert.Multiple(() =>
        {
            Assert.That(memberResponse.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(memberReasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.AlreadyMember));
            Assert.That(createdResponse.StatusCode, Is.EqualTo(HttpStatusCode.Created));
            Assert.That(duplicateResponse.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(duplicateReasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.InvitationAlreadyPending));
        });
    }

    [Test]
    public async Task UnlicensedAndFullOrganizationsReturnCapacityConflicts()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-capacity-owner", "owner@example.com");
        var licensedOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Licensed Organization",
            SignupTime.AddDays(1));
        var unlicensedOrganization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Unlicensed Organization",
            SignupTime.AddDays(2));
        using var factory = new LicensingWebApplicationFactory(database, SignupTime.AddDays(3));
        using var client = factory.CreateApiClient(owner.UserId);
        for (var index = 0; index < 4; index++)
        {
            using var preloadResponse = await PostInvitationAsync(
                client,
                licensedOrganization.OrganizationId,
                JsonContent.Create(new
                {
                    email = $"preload-{index}@example.com",
                    role = "member",
                }));
            Assert.That(preloadResponse.StatusCode, Is.EqualTo(HttpStatusCode.Created));
        }

        using var fullResponse = await PostInvitationAsync(
            client,
            licensedOrganization.OrganizationId,
            JsonContent.Create(new { email = "full@example.com", role = "member" }));
        using var unlicensedResponse = await PostInvitationAsync(
            client,
            unlicensedOrganization.OrganizationId,
            JsonContent.Create(new { email = "unlicensed@example.com", role = "member" }));
        var fullReasonCode = await ReadReasonCodeAsync(fullResponse);
        var unlicensedReasonCode = await ReadReasonCodeAsync(unlicensedResponse);

        Assert.Multiple(() =>
        {
            Assert.That(fullResponse.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(fullReasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.SeatCapacityReached));
            Assert.That(unlicensedResponse.StatusCode, Is.EqualTo(HttpStatusCode.Conflict));
            Assert.That(unlicensedReasonCode,
                Is.EqualTo(OrganizationInvitationReasonCodes.NoActiveSeatCapacity));
        });
    }

    [Test]
    public async Task CancelledRequestRollsBackInvitationAndAudit()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpAsync(database, "invitation-cancel-owner", "owner@example.com");
        var organization = await CreateOrganizationAsync(
            database,
            owner.UserId,
            "Invitation Cancel Organization",
            SignupTime.AddDays(1));
        var baseline = await CaptureWritesAsync(database);
        var gate = new DatabaseCommandGate();
        var saveGateInterceptor = new SavedChangesGateInterceptor(gate);
        var path = InvitationPath(organization.OrganizationId);
        var completionObserver = new RequestCompletionObserver(HttpMethods.Post, path);
        using var factory = new LicensingWebApplicationFactory(
            database,
            SignupTime.AddDays(2),
            useTestAuthentication: true,
            requestCompletionObserver: completionObserver,
            interceptors: [saveGateInterceptor]);
        using var client = factory.CreateApiClient(owner.UserId);
        using var cancellation = new CancellationTokenSource();

        var responseTask = PostInvitationAsync(
            client,
            organization.OrganizationId,
            JsonContent.Create(new { email = "cancelled@example.com", role = "member" }),
            cancellation.Token);
        await gate.WaitUntilReachedAsync();
        try
        {
            cancellation.Cancel();
            Assert.ThrowsAsync<TaskCanceledException>(async () => await responseTask);
        }
        finally
        {
            gate.Release();
        }

        await completionObserver.WaitUntilCompletedAsync();
        Assert.Multiple(() =>
        {
            Assert.That(completionObserver.WasCanceled, Is.True);
            Assert.That(saveGateInterceptor.CancellationObserved, Is.True);
        });
        Assert.That(await CaptureWritesAsync(database), Is.EqualTo(baseline));
    }

    private static Task<HttpResponseMessage> PostInvitationAsync(
        HttpClient client,
        Guid organizationId,
        HttpContent content,
        CancellationToken cancellationToken = default)
    {
        return PostInvitationAsync(client, organizationId.ToString(), content, cancellationToken);
    }

    private static async Task<HttpResponseMessage> PostInvitationAsync(
        HttpClient client,
        string organizationId,
        HttpContent content,
        CancellationToken cancellationToken = default)
    {
        var token = await AntiforgeryTestClient.GetTokenAsync(client);
        using var request = new HttpRequestMessage(
            HttpMethod.Post,
            InvitationPath(organizationId))
        {
            Content = content,
        };
        AntiforgeryTestClient.AddToken(request, token);
        return await client.SendAsync(request, cancellationToken);
    }

    private static string InvitationPath(Guid organizationId)
    {
        return InvitationPath(organizationId.ToString());
    }

    private static string InvitationPath(string organizationId)
    {
        return $"/api/organizations/{organizationId}/invitations";
    }

    private static async Task<string?> ReadReasonCodeAsync(HttpResponseMessage response)
    {
        var content = await response.Content.ReadAsStringAsync();
        using var body = JsonDocument.Parse(content);
        return body.RootElement.GetProperty("reasonCode").GetString();
    }

    private static async Task<InvitationWriteSnapshot> CaptureWritesAsync(
        PostgresTestDatabase database)
    {
        await using var context = database.CreateContext();
        return new InvitationWriteSnapshot(
            await context.OrganizationInvitations.CountAsync(),
            await context.AuditRecords.CountAsync(
                record => record.Action == AuditAction.OrganizationInvitationCreated),
            await context.OutboxMessages.CountAsync());
    }

    private sealed record InvitationWriteSnapshot(
        int InvitationCount,
        int AuditCount,
        int OutboxCount);
}
